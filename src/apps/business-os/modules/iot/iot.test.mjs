// IoT module contract test. Asserts the schema (RFC 0011 added iot_dashboards +
// iot_widgets), module.json/registry consistency, and that index.js bundles
// cleanly. The module's interactive behavior is verified far more thoroughly
// against the real shared BOS components via Playwright; this guards the
// static contract + catches import/syntax regressions in CI.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFile } from 'node:fs/promises';
import { readFileSync } from 'node:fs';
import { Buffer } from 'node:buffer';
import { fileURLToPath } from 'node:url';

import { createRequire } from 'node:module';
import { homedir } from 'node:os';
import { join as joinPath } from 'node:path';
// Lader (OLDTEST/FIX5): esbuild aus dem Klon, sonst ESBUILD_MODULE, sonst aus dem Haupt-Checkout
// ~/Documents/ctox/src/apps/business-os/node_modules (wie PLAYWRIGHT_MODULE in den v4-api-Tests). Nicht installieren.
const requireDev = createRequire(import.meta.url);
function loadDev(name, envVar) {
  try { return requireDev(name); } catch {
    return requireDev(process.env[envVar] || joinPath(homedir(), 'Documents/ctox/src/apps/business-os/node_modules', name));
  }
}
const { build } = loadDev('esbuild', 'ESBUILD_MODULE');

// Pure helpers exported for tests (no DOM, no RxDB). Importing index.js only
// loads the shared context-menu/dialogs modules, which touch the DOM lazily.
// v4: index.js importiert 'ctox:shared/…' (nur über den Service Worker auflösbar) und die Locales als
// JSON-Module → esbuild-Bündel mit demselben Umschreiben wie der SW (§16.15).
const ctoxSharedPlugin = {
  name: 'ctox-shared',
  setup(b) {
    b.onResolve({ filter: /^ctox:shared\// }, (args) => ({ path: fileURLToPath(new URL(`../../shared/${args.path.slice('ctox:shared/'.length)}`, import.meta.url)) }));
  },
};
const bundled = await build({
  entryPoints: [fileURLToPath(new URL('./index.js', import.meta.url))],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  write: false,
  logLevel: 'silent',
  plugins: [ctoxSharedPlugin],
});
const [{ text: bundledSource }] = bundled.outputFiles;
const bundledModule = await import(`data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`);
const {
  assetMatchesBand,
  filterAssetRows,
  countsForAssets,
  resolveMainState,
  widgetsForSelection,
} = bundledModule;

const readModuleFile = (rel) => readFileSync(new URL(rel, import.meta.url), 'utf8');
const indexHtml = readModuleFile('./index.html');
const indexJs = readModuleFile('./index.js');
const indexCss = readModuleFile('./index.css');

// v4 (§16.13): collections.schema.json ist die einzige Schemaquelle (schema.js gelöscht, inhaltsgleich).
const schemaDoc = JSON.parse(await readFile(new URL('./collections.schema.json', import.meta.url), 'utf8'));
const { collections } = schemaDoc;
const migrationStrategies = schemaDoc.migration_strategies || {};

// collections.schema.json owns all iot_* collections; assert against it, do not redefine.
const expectedCollections = [
  'iot_agent_status',
  'iot_agents',
  'iot_alarms',
  'iot_asset_types',
  'iot_assets',
  'iot_attributes',
  'iot_dashboards',
  'iot_datapoints',
  'iot_realms',
  'iot_rulesets',
  'iot_widgets',
];

assert.deepEqual(
  Object.keys(collections).slice().sort(),
  expectedCollections.slice().sort(),
  'schema.js owns exactly the expected iot_* collections',
);

// Every collection is a well-formed RxDB schema with the house index envelope.
for (const [name, schema] of Object.entries(collections)) {
  assert.equal(schema.primaryKey, 'id', `${name} primaryKey is id`);
  assert.ok(schema.properties.id, `${name} has id`);
  assert.ok(schema.properties.updated_at_ms, `${name} has updated_at_ms`);
  assert.ok(Array.isArray(schema.required) && schema.required.includes('id'), `${name} requires id`);
  // Alle Versionen 0 → keine Migration nötig (schema.js hatte je Collection eine leere Strategie).
  assert.ok(schema.version === 0 || migrationStrategies[name], `${name} is version 0 or has a migration strategy`);
}

// RFC 0011 — the two new collections carry the automation-widget fields.
assert.ok(collections.iot_dashboards.properties.scope, 'iot_dashboards has scope');
assert.ok(collections.iot_dashboards.required.includes('name'), 'iot_dashboards requires name');
for (const field of ['dashboard_id', 'signal_ref', 'cond_text', 'action_prompt', 'trigger_code', 'render_code', 'trigger_status', 'x', 'y', 'w', 'h']) {
  assert.ok(collections.iot_widgets.properties[field], `iot_widgets has ${field}`);
}
assert.ok(collections.iot_widgets.required.includes('dashboard_id'), 'iot_widgets requires dashboard_id');

// Composite index sanity preserved from the engine collections.
assert.ok(collections.iot_alarms.indexes.includes('severity'));
assert.ok(
  collections.iot_agent_status.indexes.some(
    (index) => Array.isArray(index) && index.join('|') === 'agent_id|updated_at_ms',
  ),
  'iot_agent_status has agent_id|updated_at_ms composite index',
);

// module.json + registry.json must agree, list business_commands first, then the
// iot_* read collections, and stay in the Operations category.
const moduleJson = JSON.parse(await readFile(new URL('./module.json', import.meta.url), 'utf8'));
const registryJson = JSON.parse(await readFile(new URL('../registry.json', import.meta.url), 'utf8'));

assert.equal(moduleJson.id, 'iot');
assert.equal(moduleJson.entry, 'modules/iot/index.html');
assert.equal(moduleJson.layout.shell, 'windowed');
assert.equal(moduleJson.layout.shell_contract, 'v2');
assert.equal(moduleJson.install_scope, 'core');
assert.equal(moduleJson.source, 'core');
assert.equal(moduleJson.core, true);
assert.equal(moduleJson.default_installed, true);
assert.equal(moduleJson.deletable, false);
assert.equal(moduleJson.collections[0], 'business_commands');
assert.deepEqual(
  moduleJson.collections.slice(1).slice().sort(),
  expectedCollections.slice().sort(),
  'module.json lists business_commands + every iot_* collection',
);

const registryEntry = registryJson.modules.find((mod) => mod.id === 'iot');
assert.ok(registryEntry, 'iot registry entry exists');
assert.deepEqual(registryEntry.collections, moduleJson.collections, 'registry collections match module.json');
assert.equal(registryEntry.entry, moduleJson.entry);
assert.equal(registryEntry.category, 'Operations');

// index.js bundles cleanly (bundle built above with the ctox:shared rewrite).
assert.ok(bundledSource.length > 0, 'index.js bundles to non-empty output');
assert.equal(typeof bundledModule.mount, 'function', 'bundle exposes mount');

console.log('iot module contract OK:', expectedCollections.length, 'collections, module.json/registry consistent, index.js bundles');

// ---------------------------------------------------------------------------
// IA-Karte: LEFT asset/signal tree selector (canonical grammar) + MAIN dashboard
// cards for the selection, no third column.
// ---------------------------------------------------------------------------

test('iot: IA-Karte — left selector + main dashboard, no third column', () => {
  assert.match(indexHtml, /class="ctox-workspace iot-module"/);
  assert.match(indexHtml, /class="ctox-pane iot-pane iot-left"/, 'left selector pane');
  assert.match(indexHtml, /data-iot-center/, 'main dashboard pane');
  assert.doesNotMatch(indexHtml, /data-iot-right|data-right-content/, 'no third column');
  // Exactly one column resizer (2-pane: left | resizer | main).
  const resizers = indexHtml.match(/class="ctox-column-resizer"/g) || [];
  assert.equal(resizers.length, 1, 'one resizer for a 2-pane layout');
  // Explicit grid pins + primary hard minimum.
  assert.match(indexCss, /\.iot-left\s*\{\s*grid-column:\s*1/);
  assert.match(indexCss, /\.iot-center\s*\{\s*grid-column:\s*3/);
  assert.match(indexCss, /minmax\(300px,\s*1fr\)/, 'primary column keeps a hard minimum');
  assert.match(indexCss, /grid-template-rows:\s*auto auto minmax\(0,\s*1fr\)\s*auto/, 'left pane spans header/band/well/footer rows');
});

test('iot: widget editor overlay is bounded to the shell window', () => {
  assert.match(indexJs, /host\.className = 'ctox-modal iot-modal-overlay'/);
  assert.match(indexJs, /const mountTarget = rootEl\(\) \|\| state\.root/);
  assert.match(indexJs, /if \(mountTarget\) mountTarget\.appendChild\(host\)/);
  // v4: Shell-Vorfahr durch den App-Scope ersetzt (scope-css.mjs); Kontextmenü über api.shell.menu.
  assert.match(indexCss, /\[data-app="iot"\] \.iot-modal-overlay\s*\{[\s\S]*position:\s*absolute[\s\S]*inset:\s*0/);
  assert.match(indexJs, /state\.api\?\.shell\.menu\(/);
  assert.doesNotMatch(indexJs, /createContextMenu|document\.body/);
});

test('iot: left column carries the canonical grammar markup pins', () => {
  assert.match(indexHtml, /data-pg-search/, 'grammar search input');
  // Betreiber-Direktive 31.08.2026: the shard/list switch is ONE button, not a
  // pair. It carries the CURRENT view in data-pg-view (what the shell's pane
  // grammar falls back to on every other emit) and never aria-pressed, because
  // it is an action, not a state.
  const viewToggles = indexHtml.match(/data-pg-view="(?:cards|list)"/g) || [];
  assert.equal(viewToggles.length, 1, 'exactly one view-toggle button, not two');
  const toggleTag = indexHtml.match(/<button[^>]*data-iot-view-toggle[^>]*>/)?.[0] || '';
  assert.ok(toggleTag, 'the single toggle is module-wired via data-iot-view-toggle');
  assert.doesNotMatch(toggleTag, /aria-pressed/, 'the toggle is an action, not a state');
  assert.match(toggleTag, /aria-label="[^"]+"/, 'the toggle names the view it switches to');
  // The MAIN dashboard toggle follows the same one-button rule.
  assert.doesNotMatch(indexJs, /data-view="(?:cards|list)"/, 'no two-button dashboard toggle');
  assert.match(indexJs, /viewToggleTarget\(state\.mainView\)/, 'dashboard toggle flips the view');
  assert.match(indexHtml, /data-pg-tray-toggle/, 'filter tray toggle');
  assert.match(indexHtml, /data-pg-tray\b/, 'collapsed tray');
  assert.match(indexHtml, /data-pg-reset/, 'tray reset control');
  assert.match(indexHtml, /data-pg-filter[^>]*data-pg-name="realm"/, 'realm scope is a tray filter, not a standing row');
  assert.match(indexHtml, /data-pg-footer/, 'one-line footer target');
  assert.match(indexHtml, /ctox-well/, 'recessed well');
  // No manual refresh button — the module subscribes to collections.
  assert.doesNotMatch(indexHtml, /data-action="refresh"|data-refresh/, 'no manual refresh button');
});

test('iot: counted band has >= 2 real views, each with a count target', () => {
  const bands = indexHtml.match(/data-pg-band="[^"]+"/g) || [];
  assert.ok(bands.length >= 2, `expected >= 2 view band tabs, got ${bands.length}`);
  const counts = indexHtml.match(/data-pg-count="[^"]+"/g) || [];
  assert.ok(counts.length >= 2, 'each band tab exposes a count target');
  assert.match(indexHtml, /data-pg-band="all"/);
  assert.match(indexHtml, /data-pg-band="signals"/);
  assert.match(indexHtml, /data-pg-band="alarms"/);
});

test('iot: header carries the standing Neu / Import / Export icon actions', () => {
  assert.match(indexHtml, /data-action="new"/, 'primary create action');
  assert.match(indexHtml, /data-action="import"/, 'import action');
  assert.match(indexHtml, /data-action="export"/, 'export action');
  assert.match(indexHtml, /data-action="import"[^>]*aria-label=/, 'import icon has aria-label');
  assert.match(indexHtml, /data-action="export"[^>]*aria-label=/, 'export icon has aria-label');
});

test('iot: import/export handlers are wired (JSON via Blob / file input)', () => {
  assert.match(indexJs, /=== 'new'/, 'new action handled');
  assert.match(indexJs, /=== 'import'/, 'import action handled');
  assert.match(indexJs, /=== 'export'/, 'export action handled');
  assert.match(indexJs, /new Blob\(/, 'export builds a Blob');
  assert.match(indexJs, /URL\.createObjectURL/, 'export uses an object URL');
  assert.match(indexJs, /type = 'file'/, 'import creates a file input');
  // Import goes through the real command — projections stay server-owned.
  assert.match(indexJs, /ctox\.iot\.asset\.upsert/, 'import dispatches the asset-upsert command');
});

test('iot: selecting a tree row is an in-place flip, never a tree rebuild', () => {
  assert.match(indexJs, /function selectAsset\(/, 'has a selectAsset path');
  assert.match(indexJs, /function selectSignal\(/, 'has a selectSignal path');
  assert.match(indexJs, /function applyTreeSelection\(/, 'has an in-place selection flip');
  assert.match(indexJs, /classList\.toggle\('is-selected'/, 'flips is-selected in place');
  // Neither select path may rebuild the tree.
  const selBody = indexJs.slice(indexJs.indexOf('function selectAsset('), indexJs.indexOf('function renderCreateForm('));
  assert.doesNotMatch(selBody, /renderTree\(\)/, 'selection does not rebuild the tree');
});

test('iot: band membership, filtering and counts (zeros included)', () => {
  const rows = [
    { id: 'b', name: 'Building', realm: 'master', parent_id: null, signalCount: 0, alarmOpen: false },
    { id: 'r', name: 'Room', realm: 'master', parent_id: 'b', signalCount: 2, alarmOpen: false },
    { id: 's', name: 'Sensor', realm: 'site', parent_id: 'r', signalCount: 1, alarmOpen: true },
  ];
  assert.equal(assetMatchesBand(rows[0], 'signals'), false);
  assert.equal(assetMatchesBand(rows[1], 'signals'), true);
  assert.equal(assetMatchesBand(rows[2], 'alarms'), true);
  assert.equal(assetMatchesBand(rows[0], 'all'), true);

  assert.deepEqual(countsForAssets(rows), { all: 3, signals: 2, alarms: 1 });
  assert.equal(filterAssetRows(rows, { band: 'signals' }).length, 2);
  assert.equal(filterAssetRows(rows, { band: 'alarms' }).length, 1);
  assert.equal(filterAssetRows(rows, { realm: 'site' }).length, 1);
  assert.equal(filterAssetRows(rows, { search: 'room' }).length, 1);
});

test('iot: main dashboard is revealed by selection (auto-reveal analog)', () => {
  assert.equal(resolveMainState(false), 'select', 'no selection → select prompt');
  assert.equal(resolveMainState(true), 'dashboard', 'selection → dashboard');
});

test('iot: v4 — empty data renders no state text; no readiness/sync API, commands never wait', () => {
  assert.doesNotMatch(indexJs, /subscribeCollectionReadiness|collectionReadiness|startCollection|commandBus/);
  assert.doesNotMatch(indexJs, /ctox-syncing|ctox-empty|role="status"/);
  assert.doesNotMatch(indexJs, /await dispatch\(/);
  assert.match(indexJs, /api\.query\(name, \{\}\)\.subscribe/);
  assert.match(indexJs, /api\.status\.report\(\{ level: 'error', code/);
});

test('iot: widgets are scoped by the selected asset (rollup) or signal (exact)', () => {
  const widgets = [
    { id: 'w1', signal_ref: 'a::temperature' },
    { id: 'w2', signal_ref: 'a::humidity' },
    { id: 'w3', signal_ref: 'child::temperature' },
    { id: 'w4', signal_ref: 'other::temperature' },
  ];
  // Signal selection → exactly that signal.
  const bySignal = widgetsForSelection(widgets, { signalRef: 'a::temperature' });
  assert.deepEqual(bySignal.map((w) => w.id), ['w1']);
  // Asset selection → the asset + its descendants.
  const byAsset = widgetsForSelection(widgets, { assetIds: ['a', 'child'] });
  assert.deepEqual(byAsset.map((w) => w.id), ['w1', 'w2', 'w3']);
  // No selection → nothing.
  assert.deepEqual(widgetsForSelection(widgets, {}), []);
});
