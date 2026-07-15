// IoT module contract test. Asserts the schema (RFC 0011 added iot_dashboards +
// iot_widgets), module.json/registry consistency, and that index.js bundles
// cleanly. The module's interactive behavior is verified far more thoroughly
// against the real shared BOS components via Playwright; this guards the
// static contract + catches import/syntax regressions in CI.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { Buffer } from 'node:buffer';
import { fileURLToPath } from 'node:url';

import { build } from 'esbuild';

const schemaUrl = new URL('./schema.js', import.meta.url);
const schemaSource = await readFile(schemaUrl, 'utf8');
const schemaModule = await import(`data:text/javascript;base64,${Buffer.from(schemaSource).toString('base64')}`);
const { collections, migrationStrategies } = schemaModule;

// schema.js owns all iot_* collections; assert against it, do not redefine.
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
  assert.ok(migrationStrategies[name], `${name} has a migration strategy`);
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
assert.equal(moduleJson.install_scope, 'store');
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

// index.js bundles cleanly (no missing imports / syntax errors). The shared BOS
// components it imports are real files; esbuild resolves them.
const bundled = await build({
  entryPoints: [fileURLToPath(new URL('./index.js', import.meta.url))],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  write: false,
  logLevel: 'silent',
});
const [{ text: bundledSource }] = bundled.outputFiles;
assert.ok(bundledSource.length > 0, 'index.js bundles to non-empty output');
assert.ok(bundledSource.includes('mount'), 'bundle exposes mount');

console.log('iot module contract OK:', expectedCollections.length, 'collections, module.json/registry consistent, index.js bundles');
