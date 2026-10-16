// node --test src/apps/business-os/v4/tools/manifest/gen-manifest.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import {
  generate, renderOutputs, writeOutputs, baseHash, isTestPath, isHandlerFile, normalizeCollectionsSchema, scanHandlerCode,
  listHandlers, PLATFORM_OWNER, SHELL_APP_ID, MIN_APP_API, DEFAULT_REPO, APP_ROOTS, GENERATOR_ID, PLATFORM_COLLECTIONS,
  PLATFORM_REPLACEMENTS, collectAgentPackage, extractSkillRefs, checkSkillRefs, scanHandlerSkillRefs,
  generateShellAndLibs, writeShellAndLibs, loadStandardLibs, LIB_MANIFEST_FORMAT,
  packageAssetPath, packageSrcset, ICON_ASSET_KEYS,
  DECLARATION_FILE, DECLARATION_FORMAT, validateDeclaration, triggerShapeError, collectConstants, resolveString,
  topLevelProps, scanCommandCalls, scanTriggerExports,
} from './gen-manifest.mjs';
import crypto from 'node:crypto';
import zlib from 'node:zlib';
import { spawnSync } from 'node:child_process';

const APPS_ROOT = path.join(DEFAULT_REPO, 'src/apps/business-os');
// Der Klon wird parallel von anderen Strängen bearbeitet: zwei Läufe können zwischen den Läufen geänderte App-Dateien
// sehen. Determinismus gilt, wenn zwei aufeinanderfolgende Läufe (höchstens 4 Versuche) byte-gleich sind.
let first = await generate();
let second = await generate();
for (let i = 0; i < 3 && !sameOutputs(first, second); i += 1) { first = second; second = await generate(); }
function sameOutputs(a, b) {
  const x = renderOutputs(a);
  const y = renderOutputs(b);
  return Object.keys(x).length === Object.keys(y).length && Object.keys(x).every((k) => x[k] === y[k]);
}
const appManifests = Object.values(first.manifests).filter((m) => m.app_id !== SHELL_APP_ID);
const exportsOf = (r, owner) => (owner === PLATFORM_OWNER ? r.platform[PLATFORM_OWNER].exports : r.manifests[owner]?.exports);

function appDirOf(id) {
  for (const root of APP_ROOTS) {
    const dir = path.join(APPS_ROOT, root, id);
    if (fs.existsSync(path.join(dir, 'module.json'))) return dir;
  }
  return null;
}

test('Determinismus: zweimal erzeugen ergibt byte-identische Ausgaben', () => {
  const a = renderOutputs(first);
  const b = renderOutputs(second);
  assert.deepEqual(Object.keys(a).sort(), Object.keys(b).sort());
  for (const key of Object.keys(a)) assert.equal(a[key], b[key], `abweichend: ${key}`);
});

test('ein Manifest je App unter modules/ und customer-modules/ plus shell', () => {
  const expected = APP_ROOTS.flatMap((root) => {
    const dir = path.join(APPS_ROOT, root);
    return fs.existsSync(dir) ? fs.readdirSync(dir).filter((d) => fs.existsSync(path.join(dir, d, 'module.json'))) : [];
  }).sort();
  assert.deepEqual(appManifests.map((m) => m.app_id).sort(), expected);
  assert.ok(expected.includes('outbound-lead-generation'), 'customer-modules/outbound-lead-generation erfasst');
  assert.equal(expected.length, 41);
  assert.ok(first.manifests[SHELL_APP_ID]);
});

test('jede Collection hat genau einen Eigentümer (Apps + ctox-platform)', () => {
  const owners = {};
  for (const [id, m] of Object.entries(first.manifests)) {
    for (const name of Object.keys(m.collections)) (owners[name] ||= []).push(id);
  }
  for (const name of first.platform[PLATFORM_OWNER].collections) (owners[name] ||= []).push(PLATFORM_OWNER);
  assert.deepEqual(Object.keys(owners).sort(), Object.keys(first.ownership).sort());
  for (const [name, list] of Object.entries(owners)) {
    assert.equal(list.length, 1, `${name} hat ${list.length} Eigentümer: ${list.join(', ')}`);
    assert.equal(list[0], first.ownership[name].owner);
  }
  const contract = JSON.parse(fs.readFileSync(path.join(DEFAULT_REPO, 'src/core/business_os/business_os_schema_contract.json'), 'utf8'));
  for (const name of Object.keys(contract)) assert.ok(owners[name], `Vertrags-Collection ${name} ohne Eigentümer`);
});

test('Plattform- und OLG-Eigentum nach §16.17/§16.21', () => {
  for (const name of PLATFORM_COLLECTIONS) assert.equal(first.ownership[name].owner, PLATFORM_OWNER, name);
  for (const name of ['outbound_lead_generation_adapters', 'sellify_companies', 'sellify_people', 'sellify_campaigns', 'outbound_research_policies']) {
    assert.equal(first.ownership[name].owner, 'outbound-lead-generation', name);
  }
  const browser = first.manifests.browser;
  assert.ok(!('outbound_lead_generation_adapters' in browser.collections));
  // modules/browser/V4-PORT.md OFFEN 5 (Deklaration browser/v4-manifest.json): der Browser liest die Adapter nur.
  assert.deepEqual(browser.foreign_collections.outbound_lead_generation_adapters.ops, ['read']);
  // business_module_catalog wird nie mehr deklariert, sondern über business_apps/business_app_versions gelesen.
  for (const m of Object.values(first.manifests)) {
    for (const old of Object.keys(PLATFORM_REPLACEMENTS)) assert.ok(!(old in m.foreign_collections), `${m.app_id} deklariert ${old}`);
  }
});

test('foreign ↔ exports vollständig: jede Deklaration hat ein passendes exports, kein Export ohne Nutzer oder Freigabe', () => {
  let count = 0;
  for (const [id, m] of Object.entries(first.manifests)) {
    for (const [name, f] of Object.entries(m.foreign_collections)) {
      count += 1;
      assert.notEqual(f.owner_app, id, `${id} deklariert eigene Collection ${name} als fremd`);
      assert.ok(!(name in m.collections), `${id}: ${name} zugleich eigen und fremd`);
      assert.equal(f.owner_app, first.ownership[name].owner, `${id}: ${name} falscher Eigentümer`);
      const ex = exportsOf(first, f.owner_app);
      assert.ok(ex?.[name], `${id}: ${f.owner_app} exportiert ${name} nicht`);
      for (const op of f.ops) assert.ok(ex[name].includes(op), `${id}: ${f.owner_app}.exports.${name} fehlt ${op}`);
      assert.ok(f.ops.every((op) => ['read', 'write', 'watch'].includes(op)));
    }
  }
  assert.ok(count > 0);
  assert.equal(first.stats.foreign_without_export, 0);
  for (const [id, m] of Object.entries(first.manifests)) {
    // Freigabe der Eigentümer-App (v4-manifest.json exports, §16.34 (a)) darf über die heutige Nutzung hinausgehen.
    const dir = appDirOf(id);
    const declared = dir && fs.existsSync(path.join(dir, 'v4-manifest.json')) ? JSON.parse(fs.readFileSync(path.join(dir, 'v4-manifest.json'), 'utf8')).exports || {} : {};
    for (const name of Object.keys(m.exports)) {
      const users = Object.values(first.manifests).filter((x) => x.foreign_collections[name]?.owner_app === id);
      assert.ok(users.length > 0 || declared[name], `${id} exportiert ${name} ohne Nutzer und ohne Freigabe`);
      const decl = declared[name] ? [...declared[name], 'read'] : [];
      const union = [...new Set([...users.flatMap((u) => u.foreign_collections[name].ops), ...decl])].sort();
      assert.deepEqual([...m.exports[name]].sort(), union, `${id}.exports.${name} ≠ Vereinigung der Nutzer-ops und Freigabe`);
    }
  }
});

test('Fremdzugriffs-Wünsche aus V4-HANDLERS/V4-PORT stehen im Manifest', () => {
  const want = [
    ['placements', 'accounting_invoices', ['read', 'write']],
    ['nachweise', 'accounting_invoices', ['read', 'write']],
    ['nachweise', 'planning_time_records', ['read', 'write']],
    ['invoices', 'accounting_journal_entries', ['read', 'write']],
    ['invoices', 'accounting_number_series', ['read', 'write']],
    ['consent', 'interview_scorecards', ['read', 'write']],
    ['explorer', 'desktop_files', ['read', 'write']],
    ['reports', 'business_module_reports', ['read', 'write']],
    ['support', 'communication_messages', ['read', 'watch']],
  ];
  for (const [app, name, ops] of want) {
    const f = first.manifests[app].foreign_collections[name];
    assert.ok(f, `${app}: ${name} fehlt`);
    for (const op of ops) assert.ok(f.ops.includes(op), `${app}: ${name} ohne ${op}`);
  }
  // reports/file-viewer: exklusive Liste laut V4-PORT.md
  assert.ok(!first.manifests.reports.foreign_collections.workjet_projects);
  assert.deepEqual(Object.keys(first.manifests['file-viewer'].foreign_collections).sort(), ['business_commands', 'files']);
});

test('handlers = vorhandene handlers/*.js ohne _-Präfix, jede Datei in files', () => {
  let total = 0;
  for (const m of appManifests) {
    const dir = appDirOf(m.app_id);
    const hdir = path.join(dir, 'handlers');
    const files = fs.existsSync(hdir) ? fs.readdirSync(hdir).filter((f) => f.endsWith('.js') && !f.startsWith('_')) : [];
    assert.deepEqual(Object.keys(m.handlers).sort(), files.map((f) => f.slice(0, -3)).sort(), `${m.app_id}: handlers ≠ Dateien`);
    for (const [type, rel] of Object.entries(m.handlers)) {
      assert.equal(rel, `handlers/${type}.js`);
      assert.ok(m.files[rel], `${m.app_id}: ${rel} fehlt in files`);
    }
    total += files.length;
  }
  assert.equal(first.stats.handlers_total, total);
  assert.ok(total > 0);
});

test('Trigger zeigen auf vorhandene Handler und eigene bzw. watch-freigegebene Collections', () => {
  for (const m of appManifests) {
    for (const t of m.triggers) {
      assert.ok(Object.values(m.handlers).includes(t.handler), `${m.app_id}: ${t.handler}`);
      if (t.on) {
        assert.ok(['insert', 'update', 'delete'].includes(t.on.op));
        const own = t.on.collection in m.collections;
        const f = m.foreign_collections[t.on.collection];
        assert.ok(own || f?.ops.includes('watch'), `${m.app_id}: Trigger auf ${t.on.collection} ohne watch`);
      } else {
        assert.match(t.every, /^PT\d+[MH]$/);
      }
    }
  }
  assert.ok(first.manifests.knowledge.triggers.some((t) => t.on?.collection === 'knowledge_views'));
  assert.ok(first.manifests.support.triggers.some((t) => t.handler === 'handlers/support.intake.project.js'));
});

test('capabilities_required und legacy aus dem Handler-Code', () => {
  for (const m of appManifests) {
    assert.deepEqual(m.legacy, [...m.legacy].sort());
    assert.equal(m._legacy_count, m.legacy.length);
    for (const t of m.legacy) assert.ok(m.capabilities_required.includes(`legacy.${t}`), `${m.app_id}: legacy.${t} nicht deklariert`);
    for (const c of m.capabilities_required) assert.match(c, /^[a-z_]+(\.[a-z_0-9]+)*$/, `${m.app_id}: ${c}`);
    if (!Object.keys(m.handlers).length) assert.deepEqual(m.capabilities_required, [], `${m.app_id}: Capabilities ohne Handler`);
  }
  assert.ok(first.manifests.knowledge.capabilities_required.includes('dataframe.window'));
  // TICKAPP (1fcbff780): ctox.ticket.run ist App-Logik der tickets-App, support ruft sie nicht mehr im Core auf.
  assert.ok(!first.manifests.support.capabilities_required.includes('ctox.ticket.run'));
  // Legacy-Reste je App folgen aus dem Handler-Code (Ziel 0); keine feste Erwartung an eine bestimmte App.
});

test('Pflichtfelder und Herkunft je Manifest', () => {
  for (const [id, m] of Object.entries(first.manifests)) {
    assert.equal(m.app_id, id);
    assert.equal(m._generator, GENERATOR_ID);
    assert.equal(m.min_app_api, MIN_APP_API);
    assert.equal(m.provenance.locally_modified, false);
    assert.equal(m.provenance.base_hash, baseHash(m.files), `${id}: base_hash passt nicht zu files`);
    assert.ok(Object.keys(m.files).length > 0);
    for (const key of ['handlers', 'triggers', 'command_rights', 'capabilities_required', 'legacy', 'foreign_collections', 'exports']) assert.ok(key in m, `${id}: ${key} fehlt`);
    for (const rel of Object.keys(m.files)) assert.match(m.files[rel], /^[0-9a-f]{64}$/);
    for (const [name, c] of Object.entries(m.collections)) {
      assert.ok(['full', 'window', 'projection', 'blob'].includes(c.class), `${id}.${name}: Klasse ${c.class}`);
      if (c.class !== 'full') assert.ok(c._class_proposal, `${id}.${name}: Klasse ohne Vorschlagsvermerk`);
      if (c.schema) assert.ok(c.schema.primaryKey && c._schema_source, `${id}.${name}: Schema unvollständig`);
      else assert.ok(c._todo?.length, `${id}.${name}: fehlendes Schema ohne _todo`);
      for (const key of ['client_limit', 'byte_budget', 'rights']) assert.ok(key in c, `${id}.${name}: ${key} fehlt`);
    }
    if (id !== SHELL_APP_ID) {
      for (const rel of Object.keys(m.files)) assert.ok(!isTestPath(rel), `${id}: Testdatei ${rel} in files`);
      assert.equal(m.version, JSON.parse(fs.readFileSync(path.join(appDirOf(id), 'module.json'), 'utf8')).version);
    }
  }
  assert.ok(first.manifests['outbound-lead-generation'].layout.icon_svg, 'OLG-Logo erhalten');
});

test('styles (§16.19 Nr. 6) und write_guard (§16.21): nur vorhandene Dateien, nur eigene Collections', () => {
  for (const [id, m] of Object.entries(first.manifests)) {
    assert.ok(Array.isArray(m.styles), `${id}: styles fehlt`);
    for (const rel of m.styles) {
      assert.ok(rel in m.files, `${id}: style ${rel} nicht in files`);
      assert.match(rel, /\.css$/);
    }
    if (id !== SHELL_APP_ID) assert.equal(m.styles.includes('index.css'), 'index.css' in m.files, `${id}: index.css nicht in styles`);
    for (const [name, g] of Object.entries(m.write_guard || {})) {
      assert.ok(name in m.collections, `${id}: write_guard auf fremde Collection ${name}`);
      assert.ok(Array.isArray(g.command_only_fields) && g.command_only_fields.length, `${id}.${name}: command_only_fields leer`);
      assert.ok(g._basis, `${id}.${name}: write_guard ohne Fundstelle`);
    }
  }
  assert.deepEqual(first.manifests[SHELL_APP_ID].styles, []);
  assert.ok(first.manifests['outbound-lead-generation'].write_guard.outbound_lead_generation_leads.command_only_fields.includes('field_status'));
  assert.equal(first.stats.apps_with_styles, appManifests.filter((m) => 'index.css' in m.files).length);
});

test('Shell-Manifest: Dateien laut Dateigrenze, keine Collections', () => {
  const s = first.manifests[SHELL_APP_ID];
  assert.equal(s.app_id, 'shell');
  assert.equal(s.min_app_api, '4.0');
  assert.deepEqual(s.collections, {});
  assert.ok(s.files['v4/sw.js']);
  assert.ok(s.files['v4/frame/index.js']);
  assert.ok(Object.keys(s.files).some((f) => f.startsWith('v4/shell/')));
  assert.ok(Object.keys(s.files).some((f) => f.startsWith('v4/source/')));
  assert.ok(s.files['shared/dialogs.js'] && s.files['shared/window-manager.js']);
  // §16.36 Nr. 3: nur ausgewiesene reine Code-Module sind im Daemon als ctox:shared/ importierbar.
  assert.deepEqual(s.shared_exports, ['shared/prompt-template.js']);
  for (const rel of s.shared_exports) assert.ok(s.files[rel], `shared_exports ${rel} fehlt in files`);
  for (const rel of Object.keys(s.files)) {
    assert.ok(!rel.split('/').includes('lab') && !isTestPath(rel), `shell: ${rel}`);
    assert.ok(!['shared/sync.js', 'shared/db.js'].includes(rel), `shell: Kernel-Datei ${rel}`);
  }
});

test('Testdatei-Regel: Handler-Dateien sind nie Tests', () => {
  assert.equal(isTestPath('tests/a.mjs'), true);
  assert.equal(isTestPath('core/tests/x.js'), true);
  assert.equal(isTestPath('foo.test.mjs'), true);
  assert.equal(isTestPath('registry-launch-smoke.mjs'), true);
  assert.equal(isTestPath('index.js'), false);
  assert.equal(isTestPath('core/model.js'), false);
  assert.equal(isHandlerFile('handlers/outbound.research_source.test.js'), true);
  assert.equal(isTestPath('handlers/outbound.research_source.test.js'), false);
  assert.equal(isTestPath('handlers/tests/a.test.mjs'), true);
  assert.equal(isTestPath('handlers/_lib/x.test.js'), true);
});

test('normalizeCollectionsSchema: A9-migrations, Altformat und Hüllen', () => {
  const s = { version: 1, primaryKey: 'id', type: 'object', properties: { id: { type: 'string' } } };
  const a = normalizeCollectionsSchema({ collections: { x: s, y: { syncProfile: 'eager', schema: s } }, migrations: { x: { from_ver: 0, steps: [{ to_ver: 1, operations: [] }] } } });
  assert.deepEqual(a.collections.y, s);
  assert.deepEqual(a.wrappers.y, { syncProfile: 'eager' });
  assert.equal(a.migrations.x.kind, 'a9');
  const b = normalizeCollectionsSchema({ collections: { x: s }, migration_strategies: { x: { 1: { operations: [] } } } });
  assert.equal(b.migrations.x.kind, 'strategies');
});

// ---------------------------------------------------------------- Mini-Repo

function miniRepo() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'gen-manifest-'));
  const put = (rel, text) => {
    const p = path.join(root, rel);
    fs.mkdirSync(path.dirname(p), { recursive: true });
    fs.writeFileSync(p, typeof text === 'string' ? text : JSON.stringify(text));
  };
  const schema = (props, version = 0) => ({ version, primaryKey: 'id', type: 'object', properties: { id: { type: 'string' }, ...props }, required: ['id'], additionalProperties: true });
  const M = 'src/apps/business-os/modules';
  put(`${M}/alpha/module.json`, { id: 'alpha', title: 'Alpha', version: '1.0.0', entry: 'modules/alpha/index.html', collections: ['alpha_items'], layout: { icon_asset: 'x.png' } });
  put(`${M}/alpha/collections.schema.json`, { collections: { alpha_items: schema({ name: { type: 'string' } }) } });
  put(`${M}/alpha/index.js`, 'export async function mount() {}\n');
  put(`${M}/alpha/alpha.test.mjs`, '// test\n');
  put(`${M}/alpha/handlers/alpha.item.create.js`, "export default async function handle(c, ctx) { await ctx.cap.audit.event({}); return ctx.put('alpha_items', {}); }\n");
  put(`${M}/alpha/handlers/alpha.item.test.js`, "// ctx.cap.kommentar.nicht\nexport default async function handle(c, ctx) { return ctx.cap.legacy['alpha.item.test'](c); }\n");
  put(`${M}/alpha/handlers/_lib.js`, "export const x = 1;\n");
  put(`${M}/alpha/handlers/tests/a.test.mjs`, "ctx.cap.nur.im.test();\n");
  // beta: kein schema.js (gelöscht), liest alpha_items und business_module_catalog, schreibt ctox_runtime_settings
  put(`${M}/beta/module.json`, { id: 'beta', title: 'Beta', version: '2.0.0', collections: ['alpha_items', 'business_module_catalog'] });
  put(`${M}/beta/collections.schema.json`, { collections: { alpha_items: schema({ name: { type: 'string' } }) } });
  put(`${M}/beta/index.js`, "export function mount(api) { api.put('ctox_runtime_settings', {}); return api.patch('alpha_items', { id: '1' }); }\n");
  // Kunden-App mit korrigiertem Schema unter v4/schemas
  put('src/apps/business-os/customer-modules/gamma/module.json', { id: 'gamma', title: 'Gamma', version: '0.1.0', collections: ['gamma_rows'] });
  put('src/apps/business-os/customer-modules/gamma/collections.schema.json', { collections: { gamma_rows: schema({ a: { type: 'string' } }) } });
  put('src/apps/business-os/customer-modules/gamma/index.js', 'export function mount() {}\n');
  put('src/apps/business-os/v4/schemas/gamma.collections.schema.json', { collections: { gamma_rows: schema({ a: { type: ['string', 'null'] } }, 1) }, migrations: { gamma_rows: { from_ver: 0, steps: [{ to_ver: 1, operations: [] }] } } });
  put('src/apps/business-os/v4/sw.js', '// sw\n');
  put('src/apps/business-os/v4/frame/index.js', 'export async function startShell() {}\n');
  put('src/apps/business-os/v4/frame/lab/shot.png', 'x');
  put('src/apps/business-os/shared/dialogs.js', '// d\n');
  put('src/apps/business-os/shared/sync.js', '// kernel\n');
  put('src/core/business_os/business_os_schema_contract.json', { alpha_items: schema({ name: { type: 'string' } }), orphan_rows: schema({}) });
  return root;
}

test('Mini-Repo: ohne schema.js, customer-modules, v4/schemas-Vorrang, Plattform-exports, Handler/Capabilities', async () => {
  const root = miniRepo();
  try {
    const r = await generate({ repo: root, analysis: path.join(root, 'none'), baseline: null, platformManifest: null });
    const r2 = await generate({ repo: root, analysis: path.join(root, 'none'), baseline: null, platformManifest: null });
    assert.ok(sameOutputs(r, r2), 'Mini-Repo: zwei Läufe byte-gleich');
    assert.deepEqual(Object.keys(r.manifests).sort(), ['alpha', 'beta', 'gamma', 'shell']);
    assert.deepEqual(Object.keys(r.manifests.alpha.collections), ['alpha_items']);
    assert.equal(r.manifests.beta.foreign_collections.alpha_items.owner_app, 'alpha');
    assert.deepEqual(r.manifests.beta.foreign_collections.alpha_items.ops, ['read', 'write']);
    assert.deepEqual(r.manifests.alpha.exports, { alpha_items: ['read', 'write'] });
    // Plattform: business_module_catalog → business_apps/business_app_versions; write auf ctox_runtime_settings gekappt
    const bf = r.manifests.beta.foreign_collections;
    assert.ok(!bf.business_module_catalog);
    assert.deepEqual(bf.business_apps.ops, ['read']);
    assert.deepEqual(bf.business_app_versions.ops, ['read']);
    assert.deepEqual(bf.ctox_runtime_settings.ops, ['read']);
    assert.deepEqual(bf.ctox_runtime_settings._ops_clamped, ['write']);
    assert.equal(r.stats.foreign_without_export, 0);
    // v4/schemas hat Vorrang, A9-Migrationen
    assert.equal(r.manifests.gamma.collections.gamma_rows._schema_source, 'v4/schemas/gamma.collections.schema.json');
    assert.equal(r.manifests.gamma.collections.gamma_rows.schema.version, 1);
    assert.equal(r.manifests.gamma.migrations.gamma_rows.from_ver, 0);
    assert.equal(r.manifests.gamma._source_dir, 'src/apps/business-os/customer-modules/gamma');
    // Handler, Capabilities, Legacy
    assert.deepEqual(r.manifests.alpha.handlers, { 'alpha.item.create': 'handlers/alpha.item.create.js', 'alpha.item.test': 'handlers/alpha.item.test.js' });
    assert.ok(r.manifests.alpha.files['handlers/alpha.item.test.js'], 'Handler mit „test“ im Typ bleibt in files');
    assert.ok(!r.manifests.alpha.files['alpha.test.mjs']);
    assert.deepEqual(r.manifests.alpha.legacy, ['alpha.item.test']);
    assert.deepEqual(r.manifests.alpha.capabilities_required, ['audit.event', 'legacy.alpha.item.test']);
    assert.equal(r.ownership.orphan_rows.owner, PLATFORM_OWNER);
    // Shell
    assert.deepEqual(Object.keys(r.manifests.shell.files).sort(), ['shared/dialogs.js', 'v4/frame/index.js', 'v4/sw.js']);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test('Mini-Repo: Schreiben löscht keine fremden Manifeste, nur eigene veraltete; --check danach sauber', async () => {
  const root = miniRepo();
  const out = path.join(root, 'out-manifests');
  const rep = path.join(root, 'out-report');
  const tool = path.join(root, 'out-tool');
  try {
    fs.mkdirSync(out, { recursive: true });
    fs.writeFileSync(path.join(out, 'fremd.manifest.json'), JSON.stringify({ app_id: 'fremd', _generator: 'von Hand' }));
    fs.writeFileSync(path.join(out, 'kaputt.manifest.json'), '{ kein json');
    fs.writeFileSync(path.join(out, 'weg.manifest.json'), JSON.stringify({ app_id: 'weg', _generator: GENERATOR_ID }));
    const r = await generate({ repo: root, analysis: path.join(root, 'none'), baseline: null, platformManifest: null });
    const pre = writeOutputs(r, { outDir: out, reportDir: rep, toolDir: tool, check: true });
    assert.ok(pre.mismatches.length > 0);
    const w = writeOutputs(r, { outDir: out, reportDir: rep, toolDir: tool });
    assert.deepEqual(w.removed, ['weg.manifest.json']);
    assert.ok(fs.existsSync(path.join(out, 'fremd.manifest.json')), 'fremdes Manifest bleibt');
    assert.ok(fs.existsSync(path.join(out, 'kaputt.manifest.json')), 'unlesbares Manifest bleibt');
    assert.ok(fs.existsSync(path.join(out, 'alpha.manifest.json')));
    assert.ok(fs.existsSync(path.join(tool, 'REPORT-M2.md')));
    const post = writeOutputs(r, { outDir: out, reportDir: rep, toolDir: tool, check: true });
    assert.deepEqual(post.mismatches, []);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test('scanHandlerCode: Alias, callCap, cap(ctx,[…]), Präfixe, Kommentare, Befehlsrechte', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'gen-manifest-scan-'));
  try {
    const h = path.join(root, 'handlers');
    fs.mkdirSync(h, { recursive: true });
    fs.writeFileSync(path.join(h, 'a.one.js'), [
      '// ctx.cap.kommentar.only()',
      '/* ctx.cap.block.only',
      '   ctx.cap.block.zwei */',
      'export default async function handle(command, ctx) {',
      '  const t = ctx.cap?.ctox?.ticket;',
      '  await t.run({});',
      "  await callCap(ctx, 'mail.server.config.get', {});",
      "  await cap(ctx, ['audit', 'event'])({});",
      '  return ctx.cap.doc.extract({});',
      '}',
    ].join('\n'));
    fs.writeFileSync(path.join(h, '_shared.js'), "export const X_COMMAND_PERMISSIONS = Object.freeze({\n  'a.one': 'x.triage',\n});\nexport const y = (ctx) => ctx.cap.doc.extract.verify({});\n");
    const s = scanHandlerCode(root, listHandlers(root));
    assert.deepEqual(Object.keys(s.capabilities), ['audit.event', 'ctox.ticket.run', 'doc.extract.verify', 'mail.server.config.get']);
    assert.deepEqual(s.rights['a.one'].roles, ['admin', 'chef']);
    assert.equal(s.rights['a.one']._permission, 'x.triage');
    assert.deepEqual(s.legacy, {});
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

// ---------------------------------------------------------------- Skills und Prompt-Templates (§16.30, Strang SKL0)

test('Agenten-Paket: collectAgentPackage liest skills/<name>/SKILL.md und prompts/<name>.md, meldet Probleme', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'gen-manifest-agent-'));
  const put = (rel, text = 'x\n') => { fs.mkdirSync(path.dirname(path.join(root, rel)), { recursive: true }); fs.writeFileSync(path.join(root, rel), text); };
  try {
    put('skills/parser/SKILL.md');
    put('skills/parser/references/format.md');
    put('skills/aaa-first/SKILL.md');
    put('skills/leer/references/nur.md');
    put('skills/Bad_Name/SKILL.md');
    put('skills/smoke-check/SKILL.md');
    put('skills/test/SKILL.md');
    put('prompts/issue.md');
    put('prompts/source-adapter.md');
    put('prompts/Gross.md');
    put('prompts/notiz.txt');
    put('prompts/test-run.md');
    const r = collectAgentPackage(root);
    assert.deepEqual(r.skills, [{ name: 'aaa-first', path: 'skills/aaa-first/SKILL.md' }, { name: 'parser', path: 'skills/parser/SKILL.md' }, { name: 'smoke-check', path: 'skills/smoke-check/SKILL.md' }]);
    assert.deepEqual(r.prompts, [{ name: 'issue', path: 'prompts/issue.md' }, { name: 'source-adapter', path: 'prompts/source-adapter.md' }]);
    assert.deepEqual(r.problems.map((p) => `${p.kind}:${p.name}`).sort(), [
      'excluded_by_test_rule:test', 'excluded_by_test_rule:test-run', 'invalid_prompt_name:Gross', 'invalid_skill_name:Bad_Name', 'skill_without_file:leer',
    ]);
    assert.deepEqual(collectAgentPackage(path.join(root, 'fehlt')), { skills: [], prompts: [], problems: [] });
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('Agenten-Paket: extractSkillRefs erkennt Skill-Kontexte, keine Beschriftungen/Kommentare', () => {
  const src = [
    "await ctx.cap.agent.task.enqueue({ skill: 'news/news-aggregation', suggested_skill: 'universal-scraping' });",
    "const NEWS_SKILL = 'news-aggregation';",
    "const t = { skill: str(payload, ['skill']) ?? 'business-os-outbound-campaign-setup' };",
    "send({ required_skills: flag ? ['a', 'b'] : ['c'] });",
    "const requiredSkills = ['d'];",
    "// skill: 'kommentar'",
    "const labels = { skill: 'Skill', title: 'x' ?? 'nicht' };",
    "const kw = { skill: ['cad'] };",
    "function suggestedSkill(t) { if (t) { return 'e'; } return 'f'; }",
    "function other() { return 'kein-skill'; }",
  ].join('\n');
  assert.deepEqual(extractSkillRefs(src).map((r) => `${r.line}:${r.ref}`), [
    '1:news/news-aggregation', '1:universal-scraping', '2:news-aggregation', '3:business-os-outbound-campaign-setup',
    '4:a', '4:b', '4:c', '5:d', '9:e', '9:f',
  ]);
});

test('Agenten-Paket: checkSkillRefs – nicht deklariert und fremder Namensraum', () => {
  const refs = [
    { ref: 'news/issue', file: 'handlers/a.js', line: 1 },
    { ref: 'news/fehlt', file: 'handlers/a.js', line: 2 },
    { ref: 'outbound/drafting', file: 'handlers/b.js', line: 3 },
  ];
  assert.deepEqual(checkSkillRefs('news', [{ name: 'issue', path: 'skills/issue/SKILL.md' }], refs), [
    { kind: 'undeclared_skill', app: 'news', ref: 'news/fehlt', file: 'handlers/a.js', line: 2 },
    { kind: 'foreign_namespace', app: 'news', ref: 'outbound/drafting', file: 'handlers/b.js', line: 3 },
  ]);
});

test('Agenten-Paket: Mini-Repo – Manifest skills/prompts, files, Verstöße in skill_check und _todo; ohne Paket kein Feld', async () => {
  const root = miniRepo();
  const A = path.join(root, 'src/apps/business-os/modules/alpha');
  const put = (rel, text) => { fs.mkdirSync(path.dirname(path.join(A, rel)), { recursive: true }); fs.writeFileSync(path.join(A, rel), text); };
  try {
    put('skills/summarize/SKILL.md', '---\nname: summarize\n---\n');
    put('skills/summarize/references/style.md', 'kurz\n');
    put('prompts/summary.md', 'Fasse {{topic}} zusammen.\n');
    put('handlers/alpha.sum.js', "export default async function handle(c, ctx) {\n  await ctx.cap.agent.task.enqueue({ title: 't', instruction: 'i', payload: {}, skill: 'alpha/summarize', on_complete: 'alpha.sum', on_failed: 'alpha.sum' });\n}\n");
    put('handlers/_lib/more.js', "export const a = { skill: 'alpha/ghost' };\nexport const b = { skill: 'beta/theirs' };\n");
    put('handlers/tests/s.test.mjs', "x({ skill: 'alpha/nur-test' });\n");
    const r = await generate({ repo: root, analysis: path.join(root, 'none'), baseline: null, platformManifest: null });
    const m = r.manifests.alpha;
    assert.deepEqual(m.skills, [{ name: 'summarize', path: 'skills/summarize/SKILL.md' }]);
    assert.deepEqual(m.prompts, [{ name: 'summary', path: 'prompts/summary.md' }]);
    for (const f of ['skills/summarize/SKILL.md', 'skills/summarize/references/style.md', 'prompts/summary.md']) assert.ok(m.files[f], `files: ${f}`);
    assert.ok(!('skills' in r.manifests.beta) && !('prompts' in r.manifests.beta), 'ohne Paketdateien kein Feld');
    assert.deepEqual(r.skill_check.violations.map((v) => `${v.kind}:${v.ref}:${v.file}:${v.line}`), [
      'undeclared_skill:alpha/ghost:handlers/_lib/more.js:1',
      'foreign_namespace:beta/theirs:handlers/_lib/more.js:2',
    ]);
    assert.ok(m._todo.some((t) => t.includes('alpha/ghost') && t.includes('nicht unter skills/ deklariert')));
    assert.ok(m._todo.some((t) => t.includes('beta/theirs') && t.includes('fremder Namensraum')));
    assert.deepEqual(scanHandlerSkillRefs(A).map((x) => x.ref), ['alpha/ghost', 'beta/theirs', 'alpha/summarize']);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test('Agenten-Paket: heutiger Baum ohne Verstöße, Shell-Manifest trägt shared/prompt-template.js', () => {
  assert.deepEqual(first.skill_check.violations, []);
  assert.ok(first.manifests[SHELL_APP_ID].files['shared/prompt-template.js']);
  assert.ok(!first.manifests[SHELL_APP_ID].files['shared/prompt-template.test.mjs']);
});

// ---------------------------------------------------------------- Standardbibliotheken (§16.37)

const STD_LIBS = JSON.parse(fs.readFileSync(path.join(APPS_ROOT, 'v4/tools/isolation/standard-libs.json'), 'utf8')).libs;
const APPROVED = Object.keys(STD_LIBS).filter((k) => STD_LIBS[k].status === 'approved').sort();

test('Shell-Manifest bindet libs {name: major} aus standard-libs.json und trägt keine Bibliotheksdateien', () => {
  const s = first.manifests[SHELL_APP_ID];
  assert.deepEqual(s.libs, Object.fromEntries(APPROVED.map((k) => k.split('@'))));
  assert.deepEqual(s.libs, { 'ctox-office': '1', monaco: '0' });
  assert.ok(!Object.keys(s.files).some((f) => f.startsWith('vendor/')), 'Bibliotheken gehören nicht zum Shell-Artefakt');
});

test('je freigegebene Bibliothek ein Manifest lib:<name>: alle Dateien des Quellordners mit sha256, version major.minor', () => {
  assert.deepEqual(Object.keys(first.libs).sort(), APPROVED.map((k) => k.split('@')[0]));
  const outputs = renderOutputs(first);
  for (const key of APPROVED) {
    const [name, major] = key.split('@');
    const m = first.libs[name];
    const dir = path.join(APPS_ROOT, STD_LIBS[key].source.split(/\s+/)[0]);
    assert.equal(m._format, LIB_MANIFEST_FORMAT);
    assert.equal(m._generator, GENERATOR_ID);
    assert.equal(m.app_id, `lib:${name}`);
    assert.equal(m.kind, 'lib');
    assert.equal(m.major, major);
    assert.match(m.version, new RegExp(`^${major}\\.\\d+$`));
    assert.ok(outputs[`manifests/lib-${name}.manifest.json`], `Ausgabe lib-${name}.manifest.json`);
    const onDisk = [];
    const walkDir = (d) => { for (const n of fs.readdirSync(d)) { const a = path.join(d, n); const st = fs.lstatSync(a); if (n === '.DS_Store' || n === 'node_modules' || st.isSymbolicLink()) continue; if (st.isDirectory()) walkDir(a); else onDisk.push(path.relative(dir, a).split(path.sep).join('/')); } };
    walkDir(dir);
    assert.deepEqual(Object.keys(m.files).sort(), onDisk.sort(), `${name}: jede Datei des Quellordners`);
    for (const rel of onDisk.slice(0, 5)) {
      assert.equal(m.files[rel], crypto.createHash('sha256').update(fs.readFileSync(path.join(dir, rel))).digest('hex'), rel);
    }
    assert.equal(m.provenance.base_hash, baseHash(m.files));
  }
  assert.equal(first.libs.monaco.version, '0.32', 'Monaco 0.32.1 laut vs/loader.js');
  assert.match(first.libs.monaco._version_basis, /vs\/loader\.js/);
});

test('Mini-Repo: Bibliotheken – Version aus standard-libs/Paket/Ersatz, nur approved, --shell-libs schreibt nur Shell+Libs', async () => {
  const root = miniRepo();
  const put = (rel, text) => { const p = path.join(root, 'src/apps/business-os', rel); fs.mkdirSync(path.dirname(p), { recursive: true }); fs.writeFileSync(p, typeof text === 'string' ? text : JSON.stringify(text)); };
  try {
    put('v4/tools/isolation/standard-libs.json', { libs: {
      'paket@2': { status: 'approved', source: 'vendor/paket/ (Paketversion)' },
      'fest@1': { status: 'approved', source: 'vendor/fest', version: '1.7' },
      'ohne@3': { status: 'approved', source: 'vendor/ohne/' },
      'falsch@4': { status: 'approved', source: 'vendor/falsch/' },
      'nein@1': { status: 'rejected', source: 'vendor/nein/' },
    } });
    put('vendor/paket/package.json', { version: '2.5.1' });
    put('vendor/paket/lib/a.mjs', "export const a = 1;\n");
    put('vendor/paket/tests/smoke.test.mjs', '// gehört zur Bibliothek\n');
    put('vendor/fest/x.js', 'x\n');
    put('vendor/ohne/y.js', 'y\n');
    put('vendor/falsch/package.json', { version: '5.0.0' });
    put('vendor/nein/z.js', 'z\n');
    assert.deepEqual(loadStandardLibs(path.join(root, 'src/apps/business-os')).map((l) => l.key), ['falsch@4', 'fest@1', 'ohne@3', 'paket@2']);
    const r = await generate({ repo: root, analysis: path.join(root, 'none'), baseline: null, platformManifest: null });
    assert.deepEqual(r.manifests.shell.libs, { falsch: '4', fest: '1', ohne: '3', paket: '2' });
    assert.deepEqual(Object.keys(r.libs).sort(), ['falsch', 'fest', 'ohne', 'paket']);
    assert.equal(r.libs.paket.version, '2.5');
    assert.match(r.libs.paket._version_basis, /2\.5\.1 \(package\.json\)/);
    assert.deepEqual(Object.keys(r.libs.paket.files).sort(), ['lib/a.mjs', 'package.json', 'tests/smoke.test.mjs']);
    assert.equal(r.libs.fest.version, '1.7');
    assert.equal(r.libs.ohne.version, '3.0');
    assert.ok(r.libs.ohne._todo.some((t) => /Minor unbekannt/.test(t)));
    assert.equal(r.libs.falsch.version, '4.0');
    assert.ok(r.libs.falsch._todo.some((t) => /5\.0\.0 .* passt nicht zur Bindung @4/.test(t)));
    // Vollschreiben: Lib-Manifeste gelten nicht als veraltet; --check danach sauber
    const out = path.join(root, 'out-manifests');
    const opts = { outDir: out, reportDir: path.join(root, 'rep'), toolDir: path.join(root, 'tool') };
    assert.deepEqual(writeOutputs(r, opts).removed, []);
    assert.ok(fs.existsSync(path.join(out, 'lib-paket.manifest.json')));
    assert.deepEqual(writeOutputs(r, { ...opts, check: true }).mismatches, []);
    assert.deepEqual(writeOutputs(r, opts).removed, [], 'zweiter Lauf löscht die Lib-Manifeste nicht');
    // nur Shell + Libs
    const only = path.join(root, 'only');
    const sl = generateShellAndLibs({ repo: root });
    assert.deepEqual(writeShellAndLibs(sl, { outDir: only }).written.sort(),
      ['lib-falsch.manifest.json', 'lib-fest.manifest.json', 'lib-ohne.manifest.json', 'lib-paket.manifest.json', 'shell.manifest.json']);
    assert.deepEqual(fs.readdirSync(only).sort(), ['lib-falsch.manifest.json', 'lib-fest.manifest.json', 'lib-ohne.manifest.json', 'lib-paket.manifest.json', 'shell.manifest.json']);
    assert.deepEqual(writeShellAndLibs(sl, { outDir: only, check: true }).mismatches, []);
    assert.equal(fs.readFileSync(path.join(only, 'lib-paket.manifest.json'), 'utf8'), fs.readFileSync(path.join(out, 'lib-paket.manifest.json'), 'utf8'));
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

// ---- §16.39 Nr. 6: icon_asset* als Pfad im App-Paket
test('packageAssetPath/packageSrcset: eigene Präfixe → paketrelativ, alles andere → null', () => {
  const files = { 'assets/icon/a-256.png': 'x', 'assets/icon/a-512.png': 'y', 'icon.svg': 'z' };
  assert.equal(packageAssetPath('modules/alpha/assets/icon/a-256.png', 'modules/alpha', files), 'assets/icon/a-256.png');
  assert.equal(packageAssetPath('./assets/icon/a-256.png', 'modules/alpha', files), 'assets/icon/a-256.png');
  assert.equal(packageAssetPath('/assets/icon/a-256.png', 'modules/alpha', files), 'assets/icon/a-256.png');
  assert.equal(packageAssetPath('assets/icon/a-256.png', 'modules/alpha', files), 'assets/icon/a-256.png');
  assert.equal(packageAssetPath('customer-modules/olg/icon.svg', 'customer-modules/olg', files), 'icon.svg');
  assert.equal(packageAssetPath('local-modules/olg/icon.svg', 'customer-modules/olg', files), 'icon.svg', 'Altname local-modules');
  assert.equal(packageAssetPath('local-modules/alpha/icon.svg', 'modules/alpha', files), null, 'local-modules nur für Kunden-Apps');
  assert.equal(packageAssetPath('modules/beta/icon.svg', 'modules/alpha', files), null, 'fremde App');
  assert.equal(packageAssetPath('shared/assets/x.png', 'modules/alpha', files), null, 'Shell-Datei außerhalb des Pakets');
  assert.equal(packageAssetPath('modules/alpha/../beta/icon.svg', 'modules/alpha', files), null, 'Pfadflucht');
  assert.equal(packageAssetPath('assets/icon/fehlt.png', 'modules/alpha', files), null, 'Datei fehlt im Paket');
  for (const v of ['https://x/a.png', 'data:image/png;base64,AA', '//cdn/x.png', '', null, 3]) assert.equal(packageAssetPath(v, 'modules/alpha', files), null, String(v));
  assert.deepEqual(packageSrcset('modules/alpha/assets/icon/a-256.png 1x, shared/x.png 2x, ./assets/icon/a-512.png 4x', 'modules/alpha', files),
    { value: 'assets/icon/a-256.png 1x, assets/icon/a-512.png 4x', external: ['shared/x.png'] });
  assert.deepEqual(packageSrcset('shared/x.png 1x', 'modules/alpha', files), { value: null, external: ['shared/x.png'] });
});

test('heutiger Baum: jedes icon_asset* im Manifest ist eine Datei des Pakets (files), Fremdpfade nur mit _todo', () => {
  let checked = 0;
  for (const m of appManifests) {
    const l = m.layout || {};
    for (const key of ICON_ASSET_KEYS) {
      if (l[key] == null) continue;
      const paths = key === 'icon_asset_srcset' ? l[key].split(',').map((x) => x.trim().split(/\s+/)[0]) : [l[key]];
      for (const p of paths) { assert.ok(Object.prototype.hasOwnProperty.call(m.files, p), `${m.app_id}.layout.${key}: ${p} nicht in files`); checked += 1; }
    }
    if (l._icon_asset_external) assert.ok(m._todo.some((t) => t.includes('§16.39 Nr. 6')), `${m.app_id}: Fremdpfad ohne _todo`);
  }
  assert.ok(checked >= 8, `nur ${checked} Icon-Pfade geprüft`);
  const know = first.manifests.knowledge.layout;
  assert.equal(know.icon_asset, 'assets/icon/knowledge-256.png');
  assert.equal(know.icon_asset_srcset, 'assets/icon/knowledge-256.png 1x, assets/icon/knowledge-512.png 2x, assets/icon/knowledge-1024.png 4x');
  assert.equal(first.manifests['outbound-lead-generation'].layout.icon_asset, 'assets/icon/outbound-lead-generation.svg', 'local-modules/… → Paketpfad');
  // MAN2: shared/-Icons aus module.json sind durch die Paketkopie des gewählten Bildes ersetzt (kein Fremdpfad mehr).
  for (const id of ['browser', 'creator', 'desktop', 'explorer', 'file-viewer']) {
    const l = first.manifests[id].layout;
    assert.match(String(l.icon_asset), /^assets\/icon\.(jpg|png)$/, id);
    assert.equal(l._icon_asset_external, undefined, id);
    assert.match(String(l._icon_asset_replaced?.icon_asset), /^shared\//, id);
    // MAN3: fremdes srcset entfällt; an seiner Stelle steht das srcset der abgeleiteten Paketdateien (256 px 1x, 512 px 2x).
    const ext = l.icon_asset.slice(l.icon_asset.lastIndexOf('.'));
    assert.equal(l.icon_asset_srcset, `assets/icon${ext} 1x, assets/icon@512${ext} 2x`, `${id}: srcset der Paket-Icons`);
  }
});

test('MAN2 heutiger Baum: jede App mit gewähltem Bild trägt es als Paketdatei (Shell-V2-Referenz), kein frame_palette', async () => {
  const { loadIconSelection, packageIconTarget, loadIconDerivations, ICON_PACKAGE_BASENAME, ICON_DERIVATIONS_FILE } = await import('./gen-manifest.mjs');
  const sel = await loadIconSelection(APPS_ROOT);
  const derivations = loadIconDerivations(APPS_ROOT);
  assert.ok(Object.keys(sel).length >= 39, `nur ${Object.keys(sel).length} gewählte Icons`);
  for (const m of appManifests) {
    assert.equal(m.layout.frame_palette, undefined, `${m.app_id}: frame_palette im Manifest`);
    const s = sel[m.app_id];
    if (!s) continue;
    const dir = appDirOf(m.app_id);
    const rel = path.relative(APPS_ROOT, dir).split(path.sep).join('/');
    const target = packageIconTarget(s, rel);
    assert.equal(m.layout.icon_asset, target, m.app_id);
    if (target.startsWith(`${ICON_PACKAGE_BASENAME}.`)) {
      // MAN3: abgeleitet (256 px + @512) laut Ableitungsbuch, Quelle = Auswahl-sha256.
      const d = derivations.apps[m.app_id];
      assert.ok(d, `${m.app_id}: kein Eintrag in ${ICON_DERIVATIONS_FILE}`);
      assert.equal(d.source_sha256, s.sha256, `${m.app_id}: Ableitung aus der Auswahl`);
      for (const [relOut, o] of Object.entries(d.outputs)) assert.equal(m.files[relOut], o.sha256, `${m.app_id}: ${relOut} = Ableitungsbuch`);
    } else {
      assert.equal(m.files[target], s.sha256, `${m.app_id}: eigenes Paketbild = Auswahl-sha256`);
    }
    assert.ok(!String(m.layout.icon_asset).startsWith('shared/'), m.app_id);
  }
  assert.equal(first.manifests.knowledge.layout.icon_asset, 'assets/icon/knowledge-256.png', 'eigenes Paketbild bleibt (V2 zeigt es)');
  assert.deepEqual(first.stats.icon_generated, [], 'jede App hat ein echtes Icon');
  assert.deepEqual(first.declaration_check.violations.filter((v) => v.kind === 'icon_package'), []);
});

test('MAN2 Mini-Repo: Schreiblauf kopiert das gewählte Bild ins Paket, Prüflauf meldet fehlende/abweichende Kopie', async () => {
  const root = miniRepo();
  try {
    const B = path.join(root, 'src/apps/business-os');
    const png = tinyPng(8, 8);
    const hash = crypto.createHash('sha256').update(png).digest('hex');
    fs.mkdirSync(path.join(B, 'shared/assets/sel'), { recursive: true });
    fs.writeFileSync(path.join(B, 'shared/assets/sel/alpha.png'), png);
    fs.writeFileSync(path.join(B, 'shared/operator-icon-selection.js'), `export const OPERATOR_ICON_SELECTION = Object.freeze({ alpha: Object.freeze({ asset: 'shared/assets/sel/alpha.png', sha256: '${hash}' }), beta: Object.freeze({ asset: 'shared/assets/sel/fehlt.png', sha256: '${'0'.repeat(64)}' }) });\n`);
    const mj = JSON.parse(fs.readFileSync(path.join(B, 'modules/alpha/module.json'), 'utf8'));
    fs.writeFileSync(path.join(B, 'modules/alpha/module.json'), JSON.stringify({ ...mj, layout: { icon_asset: 'shared/assets/sel/alpha.png', frame_palette: { start: '#000' } } }));
    const opts = { repo: root, analysis: path.join(root, 'none'), baseline: null, platformManifest: null };
    const dry = await generate(opts);
    assert.deepEqual(dry.declaration_check.violations.filter((v) => v.kind === 'icon_package').map((v) => v.app), ['alpha', 'beta']);
    assert.ok(!fs.existsSync(path.join(B, 'modules/alpha/assets/icon.png')), 'Prüflauf kopiert nicht');
    const r = await generate({ ...opts, syncIcons: true });
    assert.deepEqual(r.icons_copied, ['modules/alpha/assets/icon.png']);
    assert.deepEqual(r.declaration_check.violations.filter((v) => v.kind === 'icon_package').map((v) => v.app), ['beta'], 'Quelle fehlt → Verstoß');
    const l = r.manifests.alpha.layout;
    assert.equal(l.icon_asset, 'assets/icon.png');
    assert.equal(r.manifests.alpha.files['assets/icon.png'], hash);
    assert.equal(l.frame_palette, undefined, 'frame_palette wird nicht übernommen');
    assert.equal(l._icon_asset_external, undefined);
    assert.ok(!r.manifests.alpha._todo.some((t) => t.includes('§16.39 Nr. 6')), 'kein Fremdpfad-_todo');
    assert.ok(!r.stats.icon_generated.includes('alpha'));
    assert.ok(r.stats.icon_generated.includes('gamma'), 'ohne Bild: generiertes Ersatz-Icon');
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

// MAN3: kleinstes gültiges PNG (RGB, w×h, eine Farbe) für Mini-Repos; reines JS über node:zlib.
function tinyPng(w, h, rgb = [40, 120, 200]) {
  const crcTable = Array.from({ length: 256 }, (_, n) => { let c = n; for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; return c >>> 0; });
  const crc = (buf) => { let c = 0xffffffff; for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8); return (c ^ 0xffffffff) >>> 0; };
  const chunk = (type, data) => { const len = Buffer.alloc(4); len.writeUInt32BE(data.length); const td = Buffer.concat([Buffer.from(type, 'latin1'), data]); const c = Buffer.alloc(4); c.writeUInt32BE(crc(td)); return Buffer.concat([len, td, c]); };
  const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4); ihdr[8] = 8; ihdr[9] = 2;
  const raw = Buffer.alloc(h * (1 + w * 3));
  for (let y = 0; y < h; y += 1) for (let x = 0; x < w; x += 1) raw.set(rgb, y * (1 + w * 3) + 1 + x * 3);
  return Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), chunk('IHDR', ihdr), chunk('IDAT', zlib.deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]);
}

test('MAN3 imageSize liest PNG- und JPEG-Köpfe; iconOutputs/iconSrcsetFor', async () => {
  const { imageSize, iconOutputs, iconSrcsetFor } = await import('./gen-manifest.mjs');
  assert.deepEqual(imageSize(tinyPng(7, 5)), { width: 7, height: 5 });
  assert.equal(imageSize(Buffer.from('kein bild, aber lang genug fuer den kopf')), null);
  assert.deepEqual(iconOutputs('assets/icon.jpg', 1024), [{ rel: 'assets/icon.jpg', px: 256 }, { rel: 'assets/icon@512.jpg', px: 512 }]);
  assert.deepEqual(iconOutputs('assets/icon.png', 256), [{ rel: 'assets/icon.png', px: 256 }], 'keine @512-Kopie ohne Mehrauflösung');
  assert.equal(iconSrcsetFor(iconOutputs('assets/icon.jpg', 1024)), 'assets/icon.jpg 1x, assets/icon@512.jpg 2x');
  assert.equal(iconSrcsetFor(iconOutputs('assets/icon.png', 100)), null);
});

test('MAN3 heutiger Baum: Paket-Icons sind 256 px (+ 512 px), kein 1024-px-Original mehr im Paket', async () => {
  const { imageSize, loadIconDerivations } = await import('./gen-manifest.mjs');
  const d = loadIconDerivations(APPS_ROOT);
  let n = 0;
  for (const [id, e] of Object.entries(d.apps)) {
    const dir = appDirOf(id);
    for (const [rel, o] of Object.entries(e.outputs)) {
      const size = imageSize(fs.readFileSync(path.join(dir, rel)));
      assert.equal(Math.max(size.width, size.height), Math.min(o.px, e.source_px), `${id}/${rel}`);
      n += 1;
    }
  }
  assert.ok(Object.keys(d.apps).length >= 38, `nur ${Object.keys(d.apps).length} abgeleitete Apps`);
  assert.ok(n >= 76, `nur ${n} Paket-Icons`);
});

test('MAN3 Mini-Repo: --check erkennt veränderte Paket-Icons und geänderte Quelle; Verkleinern nur mit Werkzeug', async () => {
  const root = miniRepo();
  try {
    const B = path.join(root, 'src/apps/business-os');
    const { imageSize } = await import('./gen-manifest.mjs');
    const big = tinyPng(600, 600);
    const hash = crypto.createHash('sha256').update(big).digest('hex');
    fs.mkdirSync(path.join(B, 'shared/assets/sel'), { recursive: true });
    fs.writeFileSync(path.join(B, 'shared/assets/sel/alpha.png'), big);
    const selFile = path.join(B, 'shared/operator-icon-selection.js');
    fs.writeFileSync(selFile, `export const OPERATOR_ICON_SELECTION = Object.freeze({ alpha: Object.freeze({ asset: 'shared/assets/sel/alpha.png', sha256: '${hash}' }) });\n`);
    const opts = { repo: root, analysis: path.join(root, 'none'), baseline: null, platformManifest: null };
    const icons = (r) => r.declaration_check.violations.filter((v) => v.kind === 'icon_package').map((v) => v.app);
    const w = await generate({ ...opts, syncIcons: true });
    const tool = spawnSync('ffmpeg', ['-version'], { stdio: 'ignore' }).status === 0 || (process.platform === 'darwin' && fs.existsSync('/usr/bin/sips'));
    if (!tool) { assert.deepEqual(icons(w), ['alpha'], 'ohne Werkzeug: Verstoß statt stiller Kopie'); return; }
    assert.deepEqual(icons(w), []);
    assert.deepEqual(w.icons_copied.sort(), ['modules/alpha/assets/icon.png', 'modules/alpha/assets/icon@512.png']);
    assert.equal(imageSize(fs.readFileSync(path.join(B, 'modules/alpha/assets/icon.png'))).width, 256);
    assert.equal(imageSize(fs.readFileSync(path.join(B, 'modules/alpha/assets/icon@512.png'))).width, 512);
    assert.equal(w.manifests.alpha.layout.icon_asset_srcset, 'assets/icon.png 1x, assets/icon@512.png 2x');
    assert.deepEqual(icons(await generate(opts)), [], 'Prüflauf nach Schreiblauf sauber (ohne Werkzeug)');
    // Paketdatei verändert → Verstoß
    fs.writeFileSync(path.join(B, 'modules/alpha/assets/icon@512.png'), tinyPng(512, 512, [1, 2, 3]));
    assert.deepEqual(icons(await generate(opts)), ['alpha'], 'veränderte @512-Kopie');
    await generate({ ...opts, syncIcons: true });
    assert.deepEqual(icons(await generate(opts)), [], 'Schreiblauf leitet neu ab');
    // Quelle samt Auswahl neu → Ableitungsbuch veraltet → Verstoß bis zum Schreiblauf
    const big2 = tinyPng(600, 600, [200, 10, 10]);
    const hash2 = crypto.createHash('sha256').update(big2).digest('hex');
    fs.writeFileSync(path.join(B, 'shared/assets/sel/alpha.png'), big2);
    fs.writeFileSync(selFile, `export const OPERATOR_ICON_SELECTION = Object.freeze({ alpha: Object.freeze({ asset: 'shared/assets/sel/alpha.png', sha256: '${hash2}' }) });\n`);
    assert.deepEqual(icons(await generate(opts)), ['alpha'], 'neue Quelle, alte Ableitung');
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('MAN2 Shell-Manifest: Meldeschleife schreibt business_module_reports/ctox_bug_reports, ctox gibt write frei', () => {
  const s = first.manifests[SHELL_APP_ID];
  for (const n of ['business_module_reports', 'ctox_bug_reports']) {
    assert.deepEqual(s.foreign_collections[n].ops, ['read', 'write'], n);
    assert.equal(s.foreign_collections[n].owner_app, first.ownership[n].owner, n);
    for (const op of ['read', 'write']) assert.ok(exportsOf(first, s.foreign_collections[n].owner_app)[n].includes(op), `${n} ${op}`);
  }
  assert.deepEqual(s.foreign_collections.shell_preferences.ops, ['read', 'write']);
  assert.deepEqual(generateShellAndLibs().shell.foreign_collections, s.foreign_collections, '--shell-libs liefert dieselbe Deklaration');
  assert.deepEqual(first.declaration_check.violations.filter((v) => v.app === SHELL_APP_ID), []);
});

test('Mini-Repo: icon_asset wird paketrelativ, fehlende Datei → null mit _todo', async () => {
  const root = miniRepo();
  try {
    const M = path.join(root, 'src/apps/business-os/modules');
    fs.mkdirSync(path.join(M, 'beta/assets/icon'), { recursive: true });
    fs.writeFileSync(path.join(M, 'beta/assets/icon/b.png'), 'png');
    const mj = JSON.parse(fs.readFileSync(path.join(M, 'beta/module.json'), 'utf8'));
    fs.writeFileSync(path.join(M, 'beta/module.json'), JSON.stringify({ ...mj, layout: { icon_asset: 'modules/beta/assets/icon/b.png', icon_asset_srcset: 'modules/beta/assets/icon/b.png 1x' } }));
    const r = await generate({ repo: root, analysis: path.join(root, 'none'), baseline: null, platformManifest: null });
    assert.equal(r.manifests.beta.layout.icon_asset, 'assets/icon/b.png');
    assert.equal(r.manifests.beta.layout.icon_asset_srcset, 'assets/icon/b.png 1x');
    assert.ok(r.manifests.beta.files['assets/icon/b.png']);
    assert.equal(r.manifests.alpha.layout.icon_asset, null, 'x.png gibt es im Paket nicht');
    assert.deepEqual(r.manifests.alpha.layout._icon_asset_external, { icon_asset: 'x.png' });
    assert.ok(r.manifests.alpha._todo.some((t) => t.includes('§16.39 Nr. 6') && t.includes('x.png')));
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

// ---------------------------------------------------------------- Strang MAN: Deklarationsdatei, §16.40, Trigger, net.ingest

test('MAN: topLevelProps/collectConstants/resolveString – Literale, Konstanten, Objekt-Konstanten, Kurzform', () => {
  assert.deepEqual(topLevelProps("{ id: x, module: OWNER_APP, nested: { module: 'no' }, record_id }"),
    [['id', 'x'], ['module', 'OWNER_APP'], ['nested', "{ module: 'no' }"], ['record_id', 'record_id']]);
  const c = collectConstants(["export const OWNER_APP = 'olg';\nconst T = Object.freeze({ up: 'a.b.c', n: 3 });\nconst L = ['/x/', '/y/'];\nconst D = 'p';\nconst D2 = 'p';\n", "const D = 'q';\n"]);
  assert.equal(resolveString("'lit.x'", c), 'lit.x');
  assert.equal(resolveString('OWNER_APP', c), 'olg');
  assert.equal(resolveString('T.up', c), 'a.b.c');
  assert.equal(resolveString('D', c), null, 'mehrdeutige Konstante löst nicht auf');
  assert.equal(resolveString('`a${x}`', c), null);
  assert.deepEqual(c.arr.get('L'), ['/x/', '/y/']);
});

test('MAN: validateDeclaration/triggerShapeError – Form, unbekannte Schlüssel, ops, from, Trigger', () => {
  const ok = { _format: DECLARATION_FORMAT, foreign_collections: { a: ['read'] }, commands_in: [{ type: 't', from: '*' }], triggers: [{ handler: 'handlers/x.y.js', every: 'PT1H' }], multi_instance: false };
  assert.deepEqual(validateDeclaration(ok, 'app'), []);
  const kinds = (d) => validateDeclaration({ _format: DECLARATION_FORMAT, ...d }, 'app').map((e) => e.kind);
  assert.deepEqual(validateDeclaration({ foo: 1 }, 'app').map((e) => e.kind).sort(), ['declaration_format', 'declaration_unknown_key']);
  assert.deepEqual(kinds({ foreign_collections: { a: ['delete'] } }), ['declaration_ops']);
  assert.deepEqual(kinds({ commands_in: [{ type: 't', from: ['app'] }] }), ['declaration_commands_in'], 'from darf nicht die eigene App sein');
  assert.deepEqual(kinds({ commands_in: [{ type: 't', from: [] }] }), ['declaration_commands_in']);
  assert.deepEqual(kinds({ commands_out: [{ app: 'app', type: 't' }] }), ['declaration_commands_out']);
  assert.deepEqual(kinds({ multi_instance: 'nein' }), ['declaration_shape']);
  assert.equal(triggerShapeError({ handler: 'handlers/a.js', every: 'PT30S' }), 'every braucht ISO-Dauer ≥ PT1M (Stunden/Minuten)');
  assert.equal(triggerShapeError({ handler: 'handlers/_lib.js', every: 'PT1M' }), 'handler muss handlers/<typ>.js sein');
  assert.equal(triggerShapeError({ handler: 'handlers/a.js', every: 'PT1M', on: { collection: 'c', op: 'insert' } }), 'genau eines von on/every');
  assert.equal(triggerShapeError({ handler: 'handlers/a.js', on: { collection: 'c', op: 'upsert' } }), 'on braucht {collection, op: insert|update|delete}');
  assert.equal(triggerShapeError({ handler: 'handlers/a.js', every: 'PT1H30M' }), null);
});

function manRepo({ withInDecl = true, badDecl = false } = {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'gen-manifest-man-'));
  const put = (rel, text) => {
    const p = path.join(root, 'src/apps/business-os', rel);
    fs.mkdirSync(path.dirname(p), { recursive: true });
    fs.writeFileSync(p, typeof text === 'string' ? text : JSON.stringify(text, null, 2));
  };
  const schema = { version: 0, primaryKey: 'id', type: 'object', properties: { id: { type: 'string' } }, required: ['id'] };
  put('shared/context-chat.js', "export const CONTEXT_CHAT_COMMAND = 'chat.task';\nexport const APP_MODIFY_COMMAND = 'chat.modify';\nexport const CONTEXT_CHAT_OWNER = 'chat';\nexport function sendContextChat() {}\n");
  put('v4/sw.js', '//\n');
  put('v4/frame/index.js', 'export function startShell() {}\n');
  // sender: Handler mit Literal-/Konstanten-Ziel, Weiterleitung command.type über _lib, dynamisches Ziel, UI-Kontext-Chat
  put('modules/sender/module.json', { id: 'sender', title: 'S', collections: ['sender_rows'] });
  put('modules/sender/collections.schema.json', { collections: { sender_rows: schema } });
  put('modules/sender/index.js', "import { sendContextChat } from '../../shared/context-chat.js';\nexport function mount(root, api) { sendContextChat(api, {}, { canModifyApp: false }); const canModifyApp = true; api.command('recv.ui.open', { module: 'recv', record_id: 'x' }); }\n");
  put('modules/sender/handlers/_lib/forward.js', "export const RECV_APP = 'recv';\nexport async function forwardToRecv(command, ctx) {\n  const options = { id: command.id, module: RECV_APP };\n  return ctx.command(command.type, command.payload, options);\n}\n");
  put('modules/sender/handlers/recv.job.run.js', "import { forwardToRecv } from './_lib/forward.js';\nexport default async function handle(command, ctx) { return forwardToRecv(command, ctx); }\n");
  put('modules/sender/handlers/sender.ping.js', "const PING = { type: 'recv.ping' };\nexport default async function handle(command, ctx) {\n  await ctx.command(PING.type, {}, { module: 'recv', id: 'a' });\n  await ctx.command('sender.self', {}, { id: 'b' });\n  return ctx.command(command.payload.t, {}, { module: command.payload.app });\n}\n");
  put('modules/sender/handlers/sender.self.js', "export const triggers = [{ every: 'PT1H' }, { on: { collection: 'sender_rows', op: 'insert' } }];\nexport default async function handle() {}\n");
  put('modules/sender/handlers/sender.hook.js', "export default async function handle() {}\n");
  put('modules/sender/handlers/sender.hook.register.js', "const HOOK = 'sender.hook';\nconst ALIASES = ['/old/hook/'];\nexport default async function handle(c, ctx) { return ctx.cap.net.ingest.webhook.register({ path: 'hook', command: HOOK, aliases: [...ALIASES] }); }\n");
  put(`modules/sender/${DECLARATION_FILE}`, badDecl
    ? { _format: DECLARATION_FORMAT, extra_field: true }
    : { _format: DECLARATION_FORMAT, multi_instance: false, foreign_collections: { recv_rows: ['read', 'watch'] }, triggers: [{ handler: 'handlers/sender.self.js', on: { collection: 'recv_rows', op: 'update' } }] });
  // recv: nimmt recv.ping und recv.job.run von sender an, recv.ui.open nicht; gibt recv_rows mit watch frei
  put('modules/recv/module.json', { id: 'recv', title: 'R', collections: ['recv_rows'] });
  put('modules/recv/collections.schema.json', { collections: { recv_rows: schema } });
  put('modules/recv/index.js', 'export function mount() {}\n');
  for (const t of ['recv.ping', 'recv.job.run', 'recv.ui.open', 'recv.local']) put(`modules/recv/handlers/${t}.js`, 'export default async function handle() {}\n');
  put(`modules/recv/${DECLARATION_FILE}`, {
    _format: DECLARATION_FORMAT,
    exports: { recv_rows: ['read', 'watch'] },
    commands_in: withInDecl ? [{ type: 'recv.ping', from: ['sender'] }, { type: 'recv.job.run', from: '*' }] : [],
  });
  // chat: Eigentümerin der Kontext-Chat-Befehle
  put('modules/chat/module.json', { id: 'chat', title: 'C' });
  put('modules/chat/index.js', 'export function mount() {}\n');
  for (const t of ['chat.task', 'chat.modify']) put(`modules/chat/handlers/${t}.js`, 'export default async function handle() {}\n');
  put(`modules/chat/${DECLARATION_FILE}`, { _format: DECLARATION_FORMAT, commands_in: [{ type: 'chat.task', from: '*' }, { type: 'chat.modify', from: '*' }] });
  return root;
}

test('MAN: commands_out aus Code (Literal, Objekt-Konstante, Weiterleitung, UI) und commands_in aus der Deklaration', async () => {
  const root = manRepo();
  try {
    const r = await generate({ repo: root, analysis: path.join(root, 'none'), baseline: null, platformManifest: null });
    const s = r.manifests.sender;
    assert.deepEqual(s.commands_out, [
      { app: 'chat', type: 'chat.modify' },
      { app: 'chat', type: 'chat.task' },
      { app: 'recv', type: 'recv.job.run' },
      { app: 'recv', type: 'recv.ping' },
      { app: 'recv', type: 'recv.ui.open' },
    ]);
    assert.deepEqual(s._commands_out_evidence['recv recv.job.run'], ['handlers/_lib/forward.js:4']);
    assert.deepEqual(s._commands_out_evidence['recv recv.ping'], ['handlers/sender.ping.js:3']);
    assert.ok(!s.commands_out.some((c) => c.type === 'sender.self'), 'Befehl ohne module bleibt in der eigenen App');
    assert.equal(s._commands_out_unresolved.length, 1, 'dynamisches Ziel wird gemeldet');
    assert.equal(s._commands_out_unresolved[0].file, 'handlers/sender.ping.js');
    assert.deepEqual(r.manifests.recv.commands_in, [{ type: 'recv.job.run', from: '*' }, { type: 'recv.ping', from: ['sender'] }]);
    assert.deepEqual(r.manifests.chat.commands_in.map((c) => c.from), ['*', '*']);
    // genau ein Befund: recv nimmt recv.ui.open nicht an
    assert.deepEqual(r.drift.commands_unmatched, [{ from: 'sender', app: 'recv', type: 'recv.ui.open', reason: 'Ziel-App nimmt den Typ von dieser App nicht an (commands_in)' }]);
    assert.equal(r.stats.commands_unmatched, 1);
    assert.ok(s._todo.some((t) => t.includes('recv.ui.open')));
    assert.deepEqual(r.declaration_check.violations, []);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test('MAN: ohne commands_in der Ziel-App ist jeder Befehl ein Befund (Gegenprobe)', async () => {
  const root = manRepo({ withInDecl: false });
  try {
    const r = await generate({ repo: root, analysis: path.join(root, 'none'), baseline: null, platformManifest: null });
    assert.deepEqual(r.drift.commands_unmatched.filter((d) => d.app === 'recv').map((d) => d.type).sort(), ['recv.job.run', 'recv.ping', 'recv.ui.open']);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test('MAN: Trigger aus Code-Export und Deklaration, foreign watch, multi_instance, net.ingest', async () => {
  const root = manRepo();
  try {
    const r = await generate({ repo: root, analysis: path.join(root, 'none'), baseline: null, platformManifest: null });
    const s = r.manifests.sender;
    assert.deepEqual(s.triggers.map((t) => ({ on: t.on ?? null, every: t.every ?? null, src: t._source ?? DECLARATION_FILE })), [
      { on: null, every: 'PT1H', src: 'handlers/sender.self.js export triggers' },
      { on: { collection: 'recv_rows', op: 'update' }, every: null, src: DECLARATION_FILE },
      { on: { collection: 'sender_rows', op: 'insert' }, every: null, src: 'handlers/sender.self.js export triggers' },
    ]);
    assert.deepEqual(s.foreign_collections.recv_rows.ops, ['read', 'watch']);
    assert.deepEqual(r.manifests.recv.exports.recv_rows, ['read', 'watch']);
    assert.equal(s.layout.multi_instance, false);
    assert.equal(s.layout.presentation.multi_instance, false, 'der Rahmen liest presentation.multi_instance');
    assert.equal(r.manifests.recv.layout.multi_instance, undefined);
    assert.deepEqual(s._net_ingest, [{ _source: 'handlers/sender.hook.register.js:3', aliases: ['/old/hook/'], command: 'sender.hook', path: 'hook' }]);
    assert.deepEqual(s.command_rights['sender.hook'].roles, []);
    assert.equal(s._declaration, `modules/sender/${DECLARATION_FILE}`);
    assert.ok(s.files[DECLARATION_FILE], 'Deklaration ist Paketdatei');
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test('MAN: fehlerhafte Deklaration → declaration_check (Exit 1 im CLI), keine Teilübernahme', async () => {
  const root = manRepo({ badDecl: true });
  try {
    const r = await generate({ repo: root, analysis: path.join(root, 'none'), baseline: null, platformManifest: null });
    assert.deepEqual(r.declaration_check.violations.map((v) => v.kind), ['declaration_unknown_key']);
    assert.equal(r.manifests.sender.layout.multi_instance, undefined);
    assert.ok(!r.manifests.sender.foreign_collections.recv_rows?.ops.includes('watch'));
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test('MAN: heutiger Baum – Wünsche aus WUENSCHE.md stehen im Manifest', () => {
  const m = first.manifests;
  assert.deepEqual(first.declaration_check.violations, []);
  assert.equal(first.stats.commands_unmatched, 0, JSON.stringify(first.drift.commands_unmatched));
  assert.equal(first.stats.commands_in_without_handler, 0);
  // §16.40: ctox bietet die Standardbefehle allen Apps an; research → knowledge; outbound → OLG; support → tickets
  for (const type of ['ctox.chat.task', 'ctox.app.modify.request']) assert.ok(m.ctox.commands_in.some((c) => c.type === type && c.from === '*'), type);
  assert.ok(m.research.commands_out.some((c) => c.app === 'knowledge' && c.type === 'knowledge.book.ingest'));
  assert.deepEqual(m.knowledge.commands_in.find((c) => c.type === 'knowledge.book.ingest').from, ['research']);
  for (const type of ['outbound.scrape.target.upsert', 'outbound.research_source.test', 'outbound.research_source.reprobe', 'outbound.update_digest.send_now']) {
    assert.ok(m.outbound.commands_out.some((c) => c.app === 'outbound-lead-generation' && c.type === type), type);
  }
  assert.ok(m.support.commands_out.some((c) => c.app === 'tickets' && c.type === 'ctox.ticket.local.create'));
  // Trigger
  const has = (app, handler, pred) => m[app].triggers.some((t) => t.handler === handler && pred(t));
  assert.ok(has('iot', 'handlers/ctox.iot.rules.tick.js', (t) => t.every === 'PT1M'));
  assert.ok(has('iot', 'handlers/ctox.iot.signal.evaluate.js', (t) => t.on?.collection === 'iot_attributes'));
  assert.ok(has('outbound', 'handlers/outbound.tracking.register.js', (t) => t.every === 'PT1H'));
  assert.ok(has('outbound', 'handlers/outbound.scheduler.tick.js', (t) => t.every === 'PT5M'));
  assert.ok(has('outbound-lead-generation', 'handlers/outbound.contacts.email_validation_sweep.js', (t) => t.every === 'PT1H'));
  assert.ok(has('kundenpipeline', 'handlers/kundenpipeline.mail.inbound.js', (t) => t.on?.collection === 'communication_messages' && t.on.op === 'insert'));
  // foreign/exports
  assert.deepEqual(m.kundenpipeline.foreign_collections.communication_messages.ops, ['read', 'watch']);
  for (const op of ['read', 'watch', 'write']) assert.ok(m.conversations.exports.communication_messages.includes(op), op);
  for (const n of ['sellify_companies', 'sellify_people', 'sellify_campaigns', 'outbound_lead_generation_scrape_targets', 'outbound_lead_generation_scrape_runs']) {
    assert.ok(m['outbound-lead-generation'].exports[n]?.includes('read'), n);
  }
  assert.ok(!m['outbound-lead-generation'].exports.outbound_lead_generation_field_status_witnesses, 'Witnesses privat');
  // watch gibt nur die Eigentümer-Deklaration frei (outbound liest nur): Angebot OLG V4-HANDLERS.md
  assert.deepEqual(m['outbound-lead-generation'].exports.outbound_lead_generation_scrape_targets, ['read', 'watch']);
  // net.ingest: Zustelltyp ist Handler, Recht nur für die Core-Zustellung
  for (const [app, type] of [['iot', 'ctox.iot.webhook.ingest'], ['outbound', 'outbound.tracking.event']]) {
    assert.ok(m[app]._net_ingest.some((n) => n.command === type), type);
    assert.ok(type in m[app].handlers, type);
    assert.deepEqual(m[app].command_rights[type].roles, []);
  }
  // Collections und gelöschte Befehle
  assert.ok(m.knowledge.collections.knowledge_tasks);
  for (const n of ['outbound_tracking_endpoints', 'outbound_tracking_tokens', 'outbound_tracking_events']) assert.ok(m.outbound.collections[n], n);
  for (const n of ['outbound_lead_generation_scrape_targets', 'outbound_lead_generation_scrape_runs']) assert.ok(m['outbound-lead-generation'].collections[n], n);
  assert.ok(!m['outbound-lead-generation'].handlers['outbound.research_source.registry_read']);
  // Capabilities nach Kernvertrag (aus dem Code)
  assert.ok(m.iot.capabilities_required.includes('knowledge.query'));
  assert.ok(!m.iot.capabilities_required.some((c) => c.startsWith('dataframe.')));
  // OLG4 (02773df88): Personenrecherche als Agentenauftrag, kein browser.capture mehr im Handler-Code.
  assert.ok(!m['outbound-lead-generation'].capabilities_required.includes('browser.capture'));
  // multi_instance laut v4/frame/INTEGRATION.md FR2-4
  for (const app of ['knowledge', 'calendar', 'buchhaltung', 'outbound', 'threads', 'outbound-lead-generation', 'cv-print-builder']) {
    assert.equal(m[app].layout.multi_instance, false, app);
    assert.equal(m[app].layout.presentation.multi_instance, false, app);
  }
  // Konfigurationsschlüssel aus dem Code (über Hüllfunktionen _ats.js runtimeConfig/configFlag)
  assert.deepEqual(m.placements._config_keys, ['CTOX_BUSINESS_OS_AUE_REQUIRED_CREDENTIALS']);
  assert.deepEqual(m.submissions._config_keys, ['CTOX_BUSINESS_OS_REQUIRE_LEGAL_BASIS_EVIDENCE']);
  // styles aus der Deklaration
  assert.deepEqual(m.mail.styles, ['index.css', 'editor/editor.css']);
  assert.ok(m.calendar.styles.includes('vendor/event-calendar/event-calendar.min.css'));
});

test('MAN: der Generator liest keine V4-HANDLERS.md/V4-PORT.md', () => {
  const src = fs.readFileSync(new URL('./gen-manifest.mjs', import.meta.url), 'utf8');
  const code = src.split('\n').filter((l) => !l.trim().startsWith('//')).join('\n');
  assert.ok(!/['"`]V4-(HANDLERS|PORT)\.md['"`]/.test(code), 'V4-*.md als Dateiname im Code');
  assert.ok(!/\bDOC_(FOREIGN|TRIGGERS|COMMAND_RIGHTS|COLLECTION_RIGHTS)\b/.test(code));
});

test('MAN: scanCommandCalls direkt – module über Options-Variable, Konstante aus _lib', () => {
  const root = manRepo();
  try {
    const dir = path.join(root, 'src/apps/business-os/modules/sender');
    const handlers = listHandlers(dir);
    const files = ['handlers/_lib/forward.js', 'handlers/sender.ping.js'].map((rel) => fs.readFileSync(path.join(dir, rel), 'utf8'));
    const r = scanCommandCalls(dir, 'sender', handlers, collectConstants(files));
    assert.deepEqual(r.out.map((x) => `${x.app} ${x.type}`).sort(), ['recv recv.job.run', 'recv recv.ping']);
    assert.deepEqual(r.unresolved.map((u) => u.module_expr), ['command.payload.app']);
    const t = scanTriggerExports(dir, handlers);
    assert.equal(t.triggers.length, 2);
    assert.deepEqual(t.errors, []);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test('Shell-Manifest enthält den Abschluss relativer Importe (Workjet-Brücke → shared/*.mjs), auch transitiv', async () => {
  const fsMod = await import('node:fs');
  const os = await import('node:os');
  const pathMod = await import('node:path');
  const { shellImportClosure } = await import('./gen-manifest.mjs');
  const root = fsMod.mkdtempSync(pathMod.join(os.tmpdir(), 'gm-closure-'));
  const put = (rel, text) => { fsMod.mkdirSync(pathMod.dirname(pathMod.join(root, rel)), { recursive: true }); fsMod.writeFileSync(pathMod.join(root, rel), text); };
  put('v4/shell/workjet/actions/a.js', "import { x } from '../../../../shared/b.mjs?v=1';\nconst y = () => import('../../../../shared/c.js');\n");
  put('shared/b.mjs', "export { z } from './d.generated.mjs';\nexport const x = 1;\n");
  put('shared/c.js', 'export default 1;\n');
  put('shared/d.generated.mjs', 'export const z = 1;\n');
  put('shared/unused.js', 'export default 0;\n');
  const got = shellImportClosure(root, ['v4/shell/workjet/actions/a.js']);
  assert.deepEqual(got, ['shared/b.mjs', 'shared/c.js', 'shared/d.generated.mjs']);
  // Gegenprobe am echten Baum: jede relative Importkette der Workjet-Brücke liegt im erzeugten Shell-Manifest.
  const here = pathMod.dirname(new URL(import.meta.url).pathname);
  const bos = pathMod.resolve(here, '..', '..', '..');
  const shell = JSON.parse(fsMod.readFileSync(pathMod.join(bos, 'v4/manifests/shell.manifest.json'), 'utf8'));
  const seeds = Object.keys(shell.files).filter((r) => r.startsWith('v4/shell/workjet/'));
  for (const rel of shellImportClosure(bos, seeds)) assert.ok(shell.files[rel], `${rel} fehlt im Shell-Manifest`);
});
