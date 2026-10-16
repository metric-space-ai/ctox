// ABGELÖST (Strang M2, 15.10.2026): gen-manifest.mjs erfasst customer-modules/* und erzeugt
// v4/manifests/outbound-lead-generation.manifest.json selbst (Eigentum §16.21, Icon aus icon.svg über APP_EXTRAS).
// Dieses Skript würde das generierte Manifest überschreiben und `gen-manifest.mjs --check` brechen; es bricht deshalb ab.
// Der frühere Inhalt bleibt unten als Beleg der Handarbeit stehen.
throw new Error('olg-manifest.mjs ist abgelöst: node src/apps/business-os/v4/tools/manifest/gen-manifest.mjs');
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
const REPO = '/Users/michaelwelsch/Documents/ctox-shell-v4';
const BOS = path.join(REPO, 'src/apps/business-os');
const gen = await import(pathToFileURL(path.join(BOS, 'v4/tools/manifest/gen-manifest.mjs')).href);
const APP = path.join(BOS, 'customer-modules/outbound-lead-generation');
const mj = JSON.parse(fs.readFileSync(path.join(APP, 'module.json'), 'utf8'));
const cj = JSON.parse(fs.readFileSync(path.join(APP, 'collections.schema.json'), 'utf8'));
const sj = await import(pathToFileURL(path.join(APP, 'schema.js')).href);
const browserCj = JSON.parse(fs.readFileSync(path.join(BOS, 'modules/browser/collections.schema.json'), 'utf8'));
const files = gen.hashFiles(APP);
const canonEq = (a, b) => JSON.stringify(gen.canon(a)) === JSON.stringify(gen.canon(b));

const collections = {};
for (const name of mj.collections) {
  const raw = cj.collections[name];
  const wrapped = raw && raw.schema && !raw.properties;
  const schema = wrapped ? raw.schema : raw;
  const jsRaw = sj.collections[name];
  const js = jsRaw && jsRaw.schema && !jsRaw.properties ? jsRaw.schema : jsRaw;
  const entry = {
    schema, class: 'full', client_limit: null, byte_budget: null, rights: null,
    _schema_source: 'customer-modules/outbound-lead-generation/collections.schema.json',
    _owner_basis: 'eigene Collection (Präfix outbound_lead_generation_, module.json.collections, collections.schema.json module_id outbound-lead-generation)',
    _schema_js_matches_json: canonEq(js, schema),
  };
  if (wrapped) entry._schema_js_wrapper = { syncProfile: raw.syncProfile };
  if (name === 'outbound_lead_generation_leads') entry._class_proposal = 'syncProfile eager (schema.js Kommentar Owner 15.10.2026: "es gibt kein Nachladen, Daten werden gepusht") → full; client_limit/byte_budget an der Ist-Menge messen (thesen)';
  if (name === 'outbound_lead_generation_adapters') {
    const b = browserCj.collections?.[name];
    entry._todo = [
      `Eigentümer-Konflikt: der Generator (nur modules/) ordnet diese Collection heute browser zu (browser.manifest.json); browser/collections.schema.json definiert sie ${b ? (canonEq(b, schema) ? 'identisch' : 'ABWEICHEND') : 'nicht'}. Vorschlag: Eigentümer outbound-lead-generation, browser deklariert foreign_collections read/write.`,
    ];
  }
  collections[name] = entry;
}
const migrations = {
  outbound_lead_generation_adapters: {
    from_ver: 0,
    steps: [1, 2].map((v) => ({ to_ver: v, ...cj.migration_strategies.outbound_lead_generation_adapters[String(v)] })),
    migrate_fn: null,
    _source: 'customer-modules/outbound-lead-generation/collections.schema.json migration_strategies',
    _schema_js_steps: Object.keys(sj.migrationStrategies.outbound_lead_generation_adapters || {}).map(Number),
  },
};
const handlerTypes = fs.readdirSync(path.join(APP, 'handlers')).filter((f) => f.endsWith('.js')).map((f) => f.replace(/\.js$/, '')).sort();
const handlers = Object.fromEntries(handlerTypes.map((t) => [t, `handlers/${t}.js`]));
const LEGACY = ['outbound.research_source.upsert', 'outbound.research_source.generate_adapter', 'outbound.research_source.test', 'outbound.research_source.auth_assist', 'outbound.update_digest.send_now', 'outbound.lead.research_writeback', 'web_stack.person_research', 'outbound.research.adapters.reconcile'];
const capabilities_required = [
  'cap.scrape.upsert_target', 'cap.scrape.list_targets', 'cap.scrape.last_runs', 'cap.scrape.account_states',
  'cap.scrape.latest_script', 'cap.scrape.reprobe_keep_script',
  ...LEGACY.map((t) => `cap.legacy.${t}`),
].sort();
const svg = fs.readFileSync(path.join(APP, 'icon.svg'), 'utf8').trim();
const foreignNote = (e) => e;
const manifest = {
  _format: gen.MANIFEST_FORMAT,
  _generator: 'von Hand nach gen-manifest.mjs (Generator scannt nur modules/; customer-modules nicht erfasst) – Strang OLG 15.10.2026',
  _legacy_entry: mj.entry,
  _rust_basis: 'OLG-spezifisch ~20 k Z. (V4-PORT.md §11): rs:store_outbound_commands.rs (research_source/registry/policy/sellify ~3,9k von 14 278), rs:person_research_gap_closure.rs 7 305, rs:person_research_command.rs 4 418, rs:outbound_update_digest.rs 2 126, rs:outbound_field_review.rs 1 919, rs:outbound_lead_import.rs 824, rs:outbound_lead_readback.rs 576, rs:mcp_app_authority.rs (Teil), rs:external_sql_sync.rs (Plattform)',
  _suggested_capabilities: [
    { capability: 'ctx.cap.scrape', basis: 'V4-PORT.md Capability-Wünsche; registry_read/reprobe/policy.publish' },
    { capability: 'ctx.cap.browser.auth_assist', basis: 'E-befehlstypen.md outbound.research_source.auth_assist' },
    { capability: 'ctx.cap.browser.adapter', basis: 'E-befehlstypen.md outbound.research_source.{upsert,generate_adapter,test,reprobe}' },
    { capability: 'ctx.cap.mail.send', basis: 'E-befehlstypen.md outbound.update_digest.send_now' },
    { capability: 'ctx.cap.web.capture', basis: 'E-befehlstypen.md web_stack.person_research' },
    { capability: 'ctx.cap.agent.task', basis: 'business_os.chat.task Recherche-/Abgleich-Aufträge (index.js:5190, :9124)' },
    { capability: 'ctx.cap.sql.external', basis: 'E-befehlstypen.md external_sql.write (Sellify-Übergabe index.js:12212)' },
    { capability: 'ctx.cap.secret', basis: 'ctox.secret.put/list/delete (index.js:7257, :7301, :7333) – Plattform' },
  ],
  _todo: [
    'rights je Collection: module.json hat kein permissions-Feld → null; Abnahme Nicht-Admin (V4-PORT.md §13) legt die Mindestrechte fest.',
    'client_limit/byte_budget: keine Ist-Messung im Repo → null.',
    'app_ver/author/created_at setzt der Core bei der Veröffentlichung (§9.1, §9.2).',
    'schema.js ist noch Teil der Dateien; entfällt beim Port (§16.13).',
    'entry: index.js exportiert mount(ctx) (v1-Vertrag), noch nicht mount(root, api, {restored}) → Einstieg erst nach UI-Port gültig.',
    'module.json.entry zeigt auf local-modules/…; Kundeninstanzen tragen ggf. eine abweichende installierte module.json (Owner-Befund 15.10.: icon_svg und sellify_companies) – vor Aktivierung gegen die installierte Fassung auf thesen abgleichen.',
    'Befehlstypen outbound.research_source.* und outbound.update_digest.send_now sendet die App mit module "outbound"; Handler-Zuordnung je command_type über zwei Apps (outbound, outbound-lead-generation) muss der Core eindeutig routen.',
  ],
  app_id: mj.id,
  title: mj.title,
  version: mj.version,
  app_ver: null,
  author: null,
  created_at: null,
  entry: 'index.js',
  files,
  collections,
  migrations,
  foreign_collections: {
    business_commands: { owner_app: gen.PLATFORM_OWNER, ops: ['read'], _evidence: { code_read: ['index.js:803', 'index.js:815', 'index.js:7154'], sources: ['code'] } },
    sellify_companies: { owner_app: null, ops: ['read'], _evidence: { code_read: ['index.js:695', 'index.js:11601'], handler_read: ['handlers/_lib/sellify-lookup.js'], rust: ['store_outbound_commands.rs:1626', 'rxdb_peer.rs:1041'] }, _todo: 'Eigentümer offen: Projektion der externen SQL-Quelle Sellify (external_sql_sync.rs, store::local_external_data_source_declarations); keine App definiert ein Schema. Heute server demand-only (rxdb_peer.rs:1036-1046). Owner-Auflage 15.10.: muss deklariert bleiben.' },
    sellify_people: { owner_app: null, ops: ['read'], _evidence: { handler_read: ['handlers/_lib/sellify-lookup.js'], rust: ['store_outbound_commands.rs:1657'] }, _todo: 'wie sellify_companies' },
    sellify_campaigns: { owner_app: null, ops: ['read'], _evidence: { handler_read: ['handlers/_lib/sellify-lookup.js'], rust: ['store_outbound_commands.rs:1646'] }, _todo: 'wie sellify_companies' },
    outbound_research_policies: { owner_app: null, ops: ['write'], _evidence: { handler_write: ['handlers/outbound.research_policy.publish.js'], rust: ['store_outbound_commands.rs:4592'] }, _todo: 'Collection existiert heute nur nativ in business_records (kein RxDB-Schema, in keinem Manifest). Einziger Schreiber ist outbound.research_policy.publish → Vorschlag: Eigentümer outbound-lead-generation, Schema in collections.schema.json aufnehmen (UI-/Schema-Änderung außerhalb dieses Strangs).' },
    outbound_research_adapters: { owner_app: 'outbound', ops: ['write'], _evidence: { legacy_write: ['outbound.research_source.{upsert,generate_adapter,test,auth_assist} → store_outbound_commands.rs:2950-2964'] }, _todo: 'outbound.manifest.json exportiert nur read; die vier Legacy-Typen schreiben heute nativ. Beim Port: outbound exportiert write, oder die Typen wandern zu outbound.' },
  },
  exports: {
    outbound_lead_generation_adapters: ['read'],
    outbound_lead_generation_leads: ['read'],
    _todo: 'Vorschlag: browser liest/schreibt outbound_lead_generation_adapters (browser/schema.js), outbound_lead_readback.rs/MCP lesen leads; Freigaben mit den Strängen browser und mcp abstimmen.',
  },
  handlers,
  capabilities_required,
  min_app_api: gen.MIN_APP_API,
  provenance: { base_release: gen.BASE_RELEASE, base_hash: gen.baseHash(files), locally_modified: true, _note: 'Basis = App-Dateien von main@4794af03d byte-gleich; lokal hinzugefügt: handlers/** und V4-PORT.md (Strang OLG). base_hash über den aktuellen Dateistand.' },
  layout: {
    ...Object.fromEntries(['shell', 'icon_asset', 'frame_palette'].filter((k) => k in mj.layout).map((k) => [k, mj.layout[k]])),
    icon_svg: svg,
    _icon_svg_source: 'icon.svg (identisch mit assets/icon/outbound-lead-generation.svg, 738 B); module.json auf main hat kein icon_svg – Owner-Auflage 15.10.: Logo muss erhalten bleiben',
    _icon_file: 'icon.svg',
    icon: mj.icon,
    presentation: mj.presentation,
  },
  _legacy_open: LEGACY,
};
// exports may not carry a _todo key per contract shape → move it out
manifest._exports_todo = manifest.exports._todo; delete manifest.exports._todo;
const out = path.join(BOS, 'v4/manifests/outbound-lead-generation.manifest.json');
fs.writeFileSync(out, gen.stableStringify(manifest));
console.log('written', out, Object.keys(files).length, 'files', handlerTypes.length, 'handlers');
for (const [n, c] of Object.entries(collections)) console.log(n, 'schema.js==json:', c._schema_js_matches_json);
