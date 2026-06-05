import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { Buffer } from 'node:buffer';
import { fileURLToPath } from 'node:url';

import { build } from 'esbuild';

const schemaUrl = new URL('./schema.js', import.meta.url);
const schemaSource = await readFile(schemaUrl, 'utf8');
const schemaModule = await import(`data:text/javascript;base64,${Buffer.from(schemaSource).toString('base64')}`);
const { collections, migrationStrategies } = schemaModule;

// schema.js already owns the 9 iot_* collections; assert against it, do not redefine.
const expectedCollections = [
  'iot_agent_status',
  'iot_agents',
  'iot_alarms',
  'iot_asset_types',
  'iot_assets',
  'iot_attributes',
  'iot_datapoints',
  'iot_realms',
  'iot_rulesets',
];

assert.deepEqual(Object.keys(collections).sort(), expectedCollections.sort());

for (const [name, schema] of Object.entries(collections)) {
  assert.equal(schema.primaryKey, 'id', `${name} primary key`);
  assert.equal(schema.type, 'object', `${name} schema type`);
  assert.equal(schema.additionalProperties, true, `${name} allows forward-compatible properties`);
  assert.ok(Number.isInteger(schema.version), `${name} schema version`);
  assert.ok(schema.properties.id, `${name} id property`);
  assert.ok(schema.properties.updated_at_ms, `${name} updated_at_ms property`);
  assert.ok(schema.required.includes('id'), `${name} requires id`);
  assert.ok(schema.required.includes('updated_at_ms'), `${name} requires updated_at_ms`);
}

// iot uses RxDB-native `_deleted` soft-delete and indexes updated_at_ms for freshness.
for (const name of expectedCollections) {
  assert.ok(collections[name].properties._deleted, `${name} has _deleted soft-delete property`);
  assert.ok(collections[name].indexes.includes('updated_at_ms'), `${name} indexes updated_at_ms for freshness`);
}

assert.ok(collections.iot_assets.indexes.includes('parent_id'));
assert.ok(collections.iot_assets.indexes.includes('asset_type'));
assert.ok(collections.iot_assets.indexes.includes('realm'));
assert.ok(collections.iot_attributes.indexes.includes('asset_id'));
assert.ok(collections.iot_datapoints.indexes.includes('asset_id'));
assert.ok(collections.iot_alarms.indexes.includes('severity'));
assert.ok(
  collections.iot_agent_status.indexes.some(
    (index) => Array.isArray(index) && index.join('|') === 'agent_id|updated_at_ms',
  ),
  'iot_agent_status has agent_id|updated_at_ms composite index',
);
assert.ok(migrationStrategies.iot_assets, 'iot_assets migration strategy present');

const moduleJson = JSON.parse(await readFile(new URL('./module.json', import.meta.url), 'utf8'));
const registryJson = JSON.parse(await readFile(new URL('../registry.json', import.meta.url), 'utf8'));

assert.equal(moduleJson.id, 'iot');
assert.equal(moduleJson.entry, 'modules/iot/index.html');
assert.equal(moduleJson.layout.shell, 'full-workspace');
assert.ok(moduleJson.layout.icon_svg.includes('svg-iot'));
assert.equal(moduleJson.install_scope, 'store');
assert.equal(moduleJson.default_installed, false);
assert.deepEqual(moduleJson.collections, ['business_commands', ...expectedCollections.slice().sort((a, b) => moduleOrder(a) - moduleOrder(b))].length === moduleJson.collections.length
  ? moduleJson.collections
  : moduleJson.collections);
// module.json adds business_commands ahead of the 9 iot_* read collections.
assert.deepEqual(
  moduleJson.collections.slice(1).slice().sort(),
  expectedCollections.slice().sort(),
);
assert.equal(moduleJson.collections[0], 'business_commands');

const registryEntry = registryJson.modules.find((mod) => mod.id === 'iot');
assert.ok(registryEntry, 'iot registry entry exists');
assert.deepEqual(registryEntry.collections, moduleJson.collections);
assert.equal(registryEntry.entry, moduleJson.entry);
assert.equal(registryEntry.category, 'Operations');

function moduleOrder(name) {
  return moduleJson.collections.indexOf(name);
}

const bundledModule = await build({
  entryPoints: [fileURLToPath(new URL('./index.js', import.meta.url))],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  write: false,
});
const [{ text: bundledSource }] = bundledModule.outputFiles;
const { __iotTestHooks: hooks } = await import(
  `data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`
);

const now = Date.now();
const fixtures = {
  business_commands: [
    { id: 'cmd-a', module: 'iot', command_type: 'iot.alarm.ack', status: 'completed', updated_at_ms: 11 },
    { id: 'cmd-b', module: 'iot', command_type: 'iot.attribute.write', status: 'failed', error: 'rejected', updated_at_ms: 12 },
    { id: 'cmd-c', module: 'iot', command_type: 'iot.datapoints.query', status: 'pending_sync', updated_at_ms: 13 },
    { id: 'cmd-x', module: 'tickets', command_type: 'tickets.update', status: 'completed', updated_at_ms: 14 },
  ],
  iot_realms: [
    { id: 'master', realm: 'master', name: 'Master', updated_at_ms: 5 },
    { id: 'site-a', realm: 'site-a', name: 'Site A', parent_realm: 'master', updated_at_ms: 4 },
  ],
  iot_asset_types: [
    { id: 'BuildingAsset', asset_type: 'BuildingAsset', attribute_count: 2, updated_at_ms: 3 },
  ],
  iot_assets: [
    { id: 'asset-root', realm: 'master', asset_type: 'BuildingAsset', name: 'HQ Building', index_text: 'hq building', updated_at_ms: 9, location: { lat: 52.52, lng: 13.405 } },
    { id: 'asset-floor', realm: 'master', parent_id: 'asset-root', asset_type: 'FloorAsset', name: 'Floor 1', index_text: 'floor 1', updated_at_ms: 8 },
    { id: 'asset-sensor', realm: 'master', parent_id: 'asset-floor', asset_type: 'ThermostatAsset', name: 'Thermostat', index_text: 'thermostat', updated_at_ms: 7, location: { lat: 52.521, lng: 13.406 } },
    { id: 'asset-other', realm: 'site-a', asset_type: 'PlugAsset', name: 'Smart Plug', index_text: 'smart plug', updated_at_ms: 6 },
  ],
  iot_attributes: [
    { id: 'asset-sensor:temperature', realm: 'master', asset_id: 'asset-sensor', attribute_name: 'temperature', value_type: 'Number', timestamp_ms: now - 1000, data: { value: 21.5 }, updated_at_ms: 7 },
    { id: 'asset-sensor:power', realm: 'master', asset_id: 'asset-sensor', attribute_name: 'power', value_type: 'Boolean', timestamp_ms: now - 2000, data: { value: true }, updated_at_ms: 7 },
    { id: 'asset-sensor:location', realm: 'master', asset_id: 'asset-sensor', attribute_name: 'location', value_type: 'GeoPoint', timestamp_ms: now - 3000, data: { value: { lat: 52.521, lng: 13.406 } }, updated_at_ms: 7 },
  ],
  iot_datapoints: [
    {
      id: 'asset-sensor:temperature:24h',
      realm: 'master',
      asset_id: 'asset-sensor',
      attribute_name: 'temperature',
      from_ms: now - 24 * 3600 * 1000,
      to_ms: now,
      shape: 'lttb',
      point_count: 3,
      truncated: true,
      data: [
        { x: now - 24 * 3600 * 1000, y: 20.0 },
        { x: now - 12 * 3600 * 1000, y: 21.0 },
        { x: now, y: 22.0 },
      ],
      updated_at_ms: now,
    },
  ],
  iot_alarms: [
    { id: 'alarm-high', realm: 'master', title: 'Overheat', severity: 'HIGH', status: 'OPEN', status_key: 'OPEN', assignee_id: 'user-a', created_ms: now - 5000, sort_key: 'b', updated_at_ms: 9 },
    { id: 'alarm-low', realm: 'master', title: 'Drift', severity: 'LOW', status: 'ACKNOWLEDGED', status_key: 'ACKNOWLEDGED', created_ms: now - 6000, sort_key: 'a', updated_at_ms: 8 },
  ],
  iot_rulesets: [
    { id: 'rs-on', realm: 'master', name: 'Comfort', enabled: true, status_key: 'enabled', last_fired_ms: now - 7000, updated_at_ms: 5 },
    { id: 'rs-off', realm: 'master', name: 'Legacy', enabled: false, status_key: 'disabled', updated_at_ms: 4 },
  ],
  iot_agents: [
    { id: 'agent-1', realm: 'master', name: 'KNX Gateway', kind: 'KNXAgent', enabled: true, updated_at_ms: 5 },
    { id: 'agent-2', realm: 'master', name: 'HTTP Poller', kind: 'HTTPAgent', enabled: true, updated_at_ms: 4 },
  ],
  iot_agent_status: [
    { id: 'agent-1:status', realm: 'master', agent_id: 'agent-1', link_state: 'connected', last_event_ms: now - 1000, updated_at_ms: 5 },
    { id: 'agent-2:status', realm: 'master', agent_id: 'agent-2', link_state: 'error', error: 'timeout', last_event_ms: now - 2000, updated_at_ms: 4 },
  ],
};

const emptyShape = {
  business_commands: [],
  iot_realms: [],
  iot_asset_types: [],
  iot_assets: [],
  iot_attributes: [],
  iot_datapoints: [],
  iot_alarms: [],
  iot_agents: [],
  iot_agent_status: [],
  iot_rulesets: [],
};

// summarizeIotData
assert.deepEqual(hooks.summarizeIotData(fixtures), {
  realms: 2,
  assets: 4,
  attributes: 3,
  alarms: 2,
  agents: 2,
});

// buildAssetTree — parent_id hierarchy correct, root has no parent_id.
const masterAssets = hooks.filterAssetsByRealm(fixtures.iot_assets, 'master');
const tree = hooks.buildAssetTree(masterAssets);
assert.equal(tree.length, 1, 'single root in master realm');
assert.equal(tree[0].asset.id, 'asset-root');
assert.equal(tree[0].asset.parent_id, undefined);
assert.equal(tree[0].children.length, 1);
assert.equal(tree[0].children[0].asset.id, 'asset-floor');
assert.equal(tree[0].children[0].children[0].asset.id, 'asset-sensor');

// filterAssetsByRealm / filterAssets
assert.deepEqual(hooks.filterAssetsByRealm(fixtures.iot_assets, 'site-a').map((a) => a.id), ['asset-other']);
assert.deepEqual(hooks.filterAssetsByRealm(fixtures.iot_assets, '').map((a) => a.id).sort(), ['asset-floor', 'asset-other', 'asset-root', 'asset-sensor']);
assert.deepEqual(hooks.filterAssets(fixtures.iot_assets, { search: 'thermo' }).map((a) => a.id), ['asset-sensor']);

// selectedAssetContext / relatedAttributes
const ctx = hooks.selectedAssetContext('asset-sensor', fixtures);
assert.equal(ctx.asset.id, 'asset-sensor');
assert.equal(ctx.attributes.length, 3);
assert.equal(hooks.relatedAttributes('asset-sensor', fixtures).length, 3);

// formatAttributeValue
const t = (key, fallback) => fallback ?? key;
assert.equal(hooks.formatAttributeValue({ value_type: 'Number', data: { value: 21.5 } }, t), '21.5');
assert.equal(hooks.formatAttributeValue({ value_type: 'Boolean', data: { value: true } }, t), 'On');
assert.equal(hooks.formatAttributeValue({ value_type: 'Boolean', data: { value: false } }, t), 'Off');
assert.equal(hooks.formatAttributeValue({ value_type: 'GeoPoint', data: { value: { lat: 52.521, lng: 13.406 } } }, t), '52.521, 13.406');

// datapoint window + chart geometry
const window = hooks.pickDatapointWindow('asset-sensor', 'temperature', '24h', fixtures);
assert.ok(window, 'datapoint window resolved');
assert.equal(window.id, 'asset-sensor:temperature:24h');
const points = hooks.chartPointsFromDatapoint(window);
assert.equal(points.length, 3);
assert.equal(points[0].v, 20.0);
assert.equal(points[2].v, 22.0);
const geometry = hooks.buildChartGeometry(points);
assert.equal(geometry.coords.length, 3);
assert.equal(geometry.coords[0].x, 6, 'first point pinned to left padding');
assert.equal(geometry.coords[2].x, 314, 'last point pinned to right extent');
for (const coord of geometry.coords) {
  assert.ok(coord.y >= 0 && coord.y <= 120, 'y within viewport');
}
assert.equal(geometry.coords[2].y < geometry.coords[0].y, true, 'higher value plots higher (smaller y)');

// alarms
assert.deepEqual(hooks.summarizeAlarms(fixtures.iot_alarms), { total: 2, high: 1, medium: 0, low: 1 });
assert.deepEqual(hooks.groupAlarmsByStatus(fixtures.iot_alarms), { OPEN: 1, ACKNOWLEDGED: 1 });
assert.equal(hooks.alarmSeverityTone('HIGH'), 'danger');
assert.equal(hooks.alarmSeverityTone('LOW'), 'accent');
assert.equal(hooks.alarmSeverityTone('MEDIUM'), 'neutral');

// agents
const agents = hooks.joinAgentStatus(fixtures.iot_agents, fixtures.iot_agent_status);
assert.equal(agents.length, 2);
const connected = agents.find((row) => row.agent_id === 'agent-1');
assert.equal(connected.link_state, 'connected');
const errored = agents.find((row) => row.agent_id === 'agent-2');
assert.equal(errored.link_state, 'error');
assert.equal(errored.error, 'timeout');

// rulesets
assert.equal(hooks.rulesetStatusTone(fixtures.iot_rulesets[0]), 'enabled');
assert.equal(hooks.rulesetStatusTone(fixtures.iot_rulesets[1]), 'disabled');

// commands
assert.deepEqual(
  hooks.summarizeIotCommands(fixtures.business_commands.filter((cmd) => cmd.module === 'iot')),
  { pending: 1, completed: 1, failed: 1 },
);

// syncReady gate
assert.equal(hooks.syncReady({ collections: emptyShape, diagnostics: { error: '' } }), false);
assert.equal(hooks.syncReady({ collections: fixtures, diagnostics: { error: '' } }), true);
assert.equal(hooks.syncReady({ collections: fixtures, diagnostics: { error: 'peer offline' } }), false);
assert.equal(hooks.syncReady({ collections: emptyShape, diagnostics: { error: '', lastLoadedAt: now } }), true);

// activation keys
assert.equal(hooks.isActivationKey('Enter'), true);
assert.equal(hooks.isActivationKey(' '), true);
assert.equal(hooks.isActivationKey('Escape'), false);

// --- Phase 6: command dispatch shapes ---
const BUILD = hooks.BUILD;
assert.equal(typeof BUILD, 'string');

assert.deepEqual(hooks.buildAttributeWriteCommand('asset-sensor', 'temperature', 21.5), {
  module: 'iot', type: 'ctox.iot.attribute.write', record_id: 'asset-sensor',
  inbound_channel: 'business_os.iot',
  payload: { asset_id: 'asset-sensor', name: 'temperature', value: 21.5 },
  client_context: { build: BUILD, surface: 'iot.attribute.write' },
});

const upsert = hooks.buildAssetUpsertCommand({ realm: 'master', asset_type: 'PlugAsset', name: 'Plug 2', parent_id: 'asset-root' });
assert.equal(upsert.type, 'ctox.iot.asset.upsert');
assert.equal(upsert.inbound_channel, 'business_os.iot');
assert.equal(upsert.record_id, '');
assert.deepEqual(upsert.payload, { realm: 'master', asset_type: 'PlugAsset', name: 'Plug 2', parent_id: 'asset-root' });

const upsertEdit = hooks.buildAssetUpsertCommand({ id: 'asset-x', realm: 'master', asset_type: 'PlugAsset', name: 'Plug X' });
assert.equal(upsertEdit.record_id, 'asset-x');
assert.equal(upsertEdit.payload.id, 'asset-x');
assert.equal('parent_id' in upsertEdit.payload, false);

assert.deepEqual(hooks.buildAssetDeleteCommand('asset-x').payload, { asset_id: 'asset-x' });
assert.equal(hooks.buildAssetDeleteCommand('asset-x').type, 'ctox.iot.asset.delete');

assert.deepEqual(hooks.buildAlarmUpdateCommand('alarm-high', 'ack').payload, { alarm_id: 'alarm-high', action: 'ack' });
assert.deepEqual(hooks.buildAlarmUpdateCommand('alarm-high', 'assign', { assignee: 'user-b' }).payload,
  { alarm_id: 'alarm-high', action: 'assign', assignee: 'user-b' });
assert.deepEqual(hooks.buildAlarmUpdateCommand('alarm-high', 'status', { status: 'CLOSED' }).payload,
  { alarm_id: 'alarm-high', action: 'status', status: 'CLOSED' });

const rs = hooks.buildRulesetSaveCommand({ realm: 'master', name: 'Comfort', enabled: true, data: { when: [] } });
assert.equal(rs.type, 'ctox.iot.ruleset.save');
assert.equal(rs.record_id, '');
assert.deepEqual(rs.payload, { realm: 'master', name: 'Comfort', enabled: true, data: { when: [] } });
assert.equal(hooks.buildRulesetSaveCommand({ id: 'rs-1', realm: 'master', name: 'C' }).record_id, 'rs-1');
assert.equal('data' in hooks.buildRulesetSaveCommand({ realm: 'master', name: 'C' }).payload, false);
assert.equal(hooks.buildRulesetSaveCommand({ realm: 'master', name: 'C' }).payload.enabled, true);

assert.deepEqual(hooks.buildRulesetToggleCommand('rs-1', false).payload, { ruleset_id: 'rs-1', enabled: false });
assert.equal(hooks.buildRulesetToggleCommand('rs-1', false).type, 'ctox.iot.ruleset.toggle');

const ag = hooks.buildAgentConfigureCommand({ realm: 'master', name: 'MQTT 1', kind: 'mqtt' });
assert.equal(ag.type, 'ctox.iot.agent.configure');
assert.deepEqual(ag.payload, { realm: 'master', name: 'MQTT 1', kind: 'mqtt', enabled: true });

const dq = hooks.buildDatapointsQueryCommand('asset-sensor', 'temperature', '24h');
assert.equal(dq.type, 'ctox.iot.datapoints.query');
assert.equal(dq.payload.asset_id, 'asset-sensor');
assert.equal(dq.payload.attribute_name, 'temperature');
assert.equal(dq.payload.shape, 'lttb');
assert.equal(typeof dq.payload.threshold, 'number');
assert.ok(dq.payload.to_ms > dq.payload.from_ms);
assert.equal(dq.record_id, `asset-sensor:temperature:${dq.payload.from_ms}:${dq.payload.to_ms}:lttb`);

for (const cmd of [upsert, rs, ag, dq, hooks.buildAlarmUpdateCommand('a', 'ack')]) {
  assert.equal(cmd.module, 'iot');
  assert.equal(cmd.inbound_channel, 'business_os.iot');
  assert.ok(String(cmd.type).startsWith('ctox.iot.'));
}

// --- ACL awareness ---
assert.equal(hooks.canModifyModuleContext({ readonly: true }), false);
assert.equal(hooks.canModifyModuleContext({ permissions: { readonly: true } }), false);
assert.equal(hooks.canModifyModuleContext({ canModifyModule: () => true }), true);
assert.equal(hooks.canModifyModuleContext({ session: { user: { is_admin: true } } }), true);
assert.equal(hooks.canModifyModuleContext({ session: { user: { role: 'chef' } } }), true);
assert.equal(hooks.canModifyModuleContext({ session: { user: { role: 'viewer' } } }), false);

// --- ruleset validation ---
assert.equal(hooks.validateRulesetDraft({ name: '', realm: 'master' }).valid, false);
assert.equal(hooks.validateRulesetDraft({ name: 'X', realm: '' }).valid, false);
assert.equal(hooks.validateRulesetDraft({ name: 'X', realm: 'master', dataText: '{bad' }).valid, false);
const okDraft = { name: 'X', realm: 'master', dataText: '{"when":[]}' };
assert.equal(hooks.validateRulesetDraft(okDraft).valid, true);
assert.deepEqual(okDraft.data, { when: [] });

// --- value coercion ---
assert.equal(hooks.coerceAttributeValue('Number', '21.5').value, 21.5);
assert.equal(hooks.coerceAttributeValue('Number', 'abc').error != null, true);
assert.equal(hooks.coerceAttributeValue('Boolean', true).value, true);
assert.equal(hooks.coerceAttributeValue('Text', 'hi').value, 'hi');

// --- command tone classification (failed surfaces honestly) ---
assert.equal(hooks.commandStatusTone('completed'), 'completed');
assert.equal(hooks.commandStatusTone('failed'), 'failed');
assert.equal(hooks.commandStatusTone('pending_sync'), 'pending');

console.log('iot schema smoke OK');
