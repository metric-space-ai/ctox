// Tests der iot-App-Handler (Strang IOTAPP): Fachverhalten gegen die Rust-Referenz src/core/iot/*.rs @ main@4794af03d.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { createCtx, run, makeCommand } from './_ctx.mjs';
import { knowledgeSim } from './_knowledge-sim.mjs';
import { validateWatcher, evaluateWatcher, generateId, zeroPad } from '../_iotcore.js';

const T = (op) => `ctox.iot.${op}`;
const thermostat = (n = 1) => ({
  asset_type: 'Thermostat',
  attributes: Array.from({ length: n }, (_, i) => ({
    name: n === 1 ? 'temp' : `a${String(i).padStart(4, '0')}`,
    value_descriptor: { name: 'number', base_type: 'Number' }, meta: { unit: 'celsius' },
  })),
});
const asset = (extra = {}) => ({ realm: 'master', asset_type: 'Thermostat', name: 'Pumpe 1', ...extra });

test('Handler rufen keine iot-Capability mehr auf (Quelltext und Laufzeit)', async () => {
  const dir = fileURLToPath(new URL('..', import.meta.url));
  for (const f of readdirSync(dir).filter((x) => x.endsWith('.js'))) {
    const text = readFileSync(`${dir}/${f}`, 'utf8');
    assert.doesNotMatch(text, /cap\.iot|'iot\./, f);
  }
});

test('generateId: 22 Zeichen Base62 (model.rs:282-307)', () => {
  const id = generateId({ uuid: () => 'ffffffff-ffff-ffff-ffff-ffffffffffff' });
  assert.equal(id.length, 22);
  assert.match(id, /^[0-9A-Za-z]{22}$/);
  assert.equal(generateId({ uuid: () => '00000000-0000-0000-0000-000000000001' }), '0000000000000000000001');
  assert.equal(zeroPad(-5, 8), '-0000005');
  assert.equal(zeroPad(42, 8), '00000042');
});

test('asset.upsert: neu mit Typ-Deskriptor, Realm aus der Sitzung, Attribute vorbelegt', async () => {
  const ctx = createCtx();
  const out = await run(ctx, T('asset.upsert'), asset({ realm: 'evil', asset_type_info: thermostat() }));
  assert.equal(out.asset.realm, 'master');
  assert.equal(out.asset.id.length, 22);
  assert.deepEqual(out.asset.attributes.temp, { name: 'temp', value_type: 'Number', value: null, timestamp: 0, meta: { unit: 'celsius' } });
  assert.deepEqual(out.projections.map((p) => p.collection), ['iot_assets', 'iot_asset_types', 'iot_attributes']);
  const row = ctx.$.doc('iot_attributes', `${out.asset.id}:temp`);
  assert.equal(row.status_key, 'Number');
  assert.equal(ctx.$.doc('iot_asset_types', 'Thermostat').attribute_count, 1);
  assert.equal(ctx.$.doc('iot_assets', out.asset.id).index_text, 'pumpe 1');
  assert.equal(ctx.$.capCalls.length, 0);
});

test('asset.upsert: Update behält Attribute, parent_id wird überschrieben; Pflichtfelder wie serde', async () => {
  const ctx = createCtx();
  const first = await run(ctx, T('asset.upsert'), asset({ id: 'a1', parent_id: 'p', asset_type_info: thermostat() }));
  const second = await run(ctx, T('asset.upsert'), asset({ id: 'a1', name: 'Pumpe 2' }));
  assert.deepEqual(Object.keys(second.asset.attributes), ['temp']);
  assert.equal(second.asset.parent_id, null);
  assert.equal(ctx.$.doc('iot_assets', 'a1').parent_id, undefined);
  assert.equal(first.asset.parent_id, 'p');
  await assert.rejects(run(ctx, T('asset.upsert'), { asset_type: 'x', name: 'y' }), { code: 'invalid', message: 'failed to deserialize iot command payload' });
  await assert.rejects(run(ctx, T('asset.upsert'), asset({ parent_id: 7 })), { code: 'invalid' });
});

test('asset.upsert: fremder Realm = „asset not found“ (Realm-Isolation, commands.rs:617-623)', async () => {
  const ctx = createCtx({ seed: { iot_assets: [{ id: 'x1', realm: 'other', asset_type: 't', name: 'n', data: { id: 'x1', realm: 'other', attributes: {} }, updated_at_ms: 1 }] } });
  await assert.rejects(run(ctx, T('asset.upsert'), asset({ id: 'x1' })), { code: 'not_found', message: 'asset not found: x1' });
  assert.equal(ctx.$.doc('iot_assets', 'x1').realm, 'other');
  await assert.rejects(run(ctx, T('asset.delete'), { asset_id: 'x1' }), { code: 'not_found', message: 'asset not found: x1' });
});

test('asset.upsert: Attribut-Deckel MAX_ATTRIBUTES_PER_ASSET = 1000 (§2A.14)', async () => {
  const ctx = createCtx();
  const out = await run(ctx, T('asset.upsert'), asset({ id: 'big', asset_type_info: thermostat(1001) }));
  assert.equal(Object.keys(out.asset.attributes).length, 1001);
  assert.equal(ctx.$.all('iot_attributes').length, 1000);
  assert.equal(ctx.$.doc('iot_attributes', 'big:a1000'), null);
  assert.equal(ctx.$.logs.length, 1);
});

test('asset.delete: Asset und Attributzeilen weg; Pflichtfeld', async () => {
  const ctx = createCtx();
  await run(ctx, T('asset.upsert'), asset({ id: 'a1', asset_type_info: thermostat() }));
  const out = await run(ctx, T('asset.delete'), { asset_id: 'a1' });
  assert.equal(out.deleted, 'a1');
  assert.equal(ctx.$.doc('iot_assets', 'a1'), null);
  assert.equal(ctx.$.doc('iot_attributes', 'a1:temp'), null);
  await assert.rejects(run(ctx, T('asset.delete'), {}), { code: 'invalid', message: 'ctox.iot.asset.delete requires asset_id' });
  await assert.rejects(run(ctx, T('asset.delete'), { asset_id: 'a1' }), { code: 'not_found' });
});

const alarmSeed = (status, realm = 'master') => ({
  iot_alarms: [{ id: 'al1', realm, title: 'T', severity: 'MEDIUM', status: 'X', updated_at_ms: 1, created_at_ms: 5,
    data: { id: 'al1', realm, title: 'T', content: null, severity: 'Medium', status, assignee_id: null, source: 'Manual', source_id: 's', created: 5, last_modified: 5, asset_ids: [] } }],
});

test('alarm.update: Lebenszyklus und Übergangsregeln (alarms.rs:60-75)', async () => {
  const ctx = createCtx({ seed: alarmSeed('Open') });
  let out = await run(ctx, T('alarm.update'), { alarm_id: 'al1', action: 'ACK' });
  assert.equal(out.alarm.status, 'Acknowledged');
  assert.equal(ctx.$.doc('iot_alarms', 'al1').status_key, 'ACKNOWLEDGED');
  out = await run(ctx, T('alarm.update'), { alarm_id: 'al1', action: 'status', status: 'progress' });
  assert.equal(ctx.$.doc('iot_alarms', 'al1').status, 'IN_PROGRESS');
  await assert.rejects(run(ctx, T('alarm.update'), { alarm_id: 'al1', action: 'status', status: 'open' }),
    { code: 'conflict', message: 'illegal alarm status transition IN_PROGRESS -> OPEN' });
  out = await run(ctx, T('alarm.update'), { alarm_id: 'al1', action: 'assign', assignee: 'u1' });
  assert.equal(out.alarm.assignee_id, 'u1');
  assert.equal(ctx.$.doc('iot_alarms', 'al1').sort_key, '00000000000000000005');
  await run(ctx, T('alarm.update'), { alarm_id: 'al1', action: 'close' });
  await assert.rejects(run(ctx, T('alarm.update'), { alarm_id: 'al1', action: 'resolve' }), { code: 'conflict', message: 'illegal alarm status transition CLOSED -> RESOLVED' });
  await run(ctx, T('alarm.update'), { alarm_id: 'al1', action: 'close' }); // Selbstübergang erlaubt
  await assert.rejects(run(ctx, T('alarm.update'), { alarm_id: 'al1', action: 'zap' }), { code: 'invalid', message: 'unknown alarm action: zap' });
  await assert.rejects(run(ctx, T('alarm.update'), { alarm_id: 'al1', action: 'status' }), { code: 'invalid', message: 'status action requires `status`' });
  await assert.rejects(run(ctx, T('alarm.update'), { alarm_id: 'al1', action: 'status', status: 'x' }), { code: 'invalid', message: 'unknown alarm status: X' });
});

test('alarm.update: Resolved → Open (Wiedereröffnen), fremder Realm = „Alarm does not exist“', async () => {
  const ctx = createCtx({ seed: alarmSeed('Resolved') });
  const out = await run(ctx, T('alarm.update'), { alarm_id: 'al1', action: 'status', status: 'OPEN' });
  assert.equal(out.alarm.status, 'Open');
  const other = createCtx({ seed: alarmSeed('Open', 'other') });
  await assert.rejects(run(other, T('alarm.update'), { alarm_id: 'al1', action: 'ack' }), { code: 'not_found', message: 'Alarm does not exist: al1' });
});

test('ruleset.save/toggle: Vorgabe enabled=true, status_key, fehlende Regel', async () => {
  const ctx = createCtx();
  const out = await run(ctx, T('ruleset.save'), { id: 'r1', realm: 'x', name: 'Hitze', data: { name: 'Hitze' } });
  assert.deepEqual(out.ruleset, { id: 'r1', realm: 'master', name: 'Hitze', enabled: true });
  assert.equal(ctx.$.doc('iot_rulesets', 'r1').status_key, 'enabled');
  await run(ctx, T('ruleset.toggle'), { ruleset_id: 'r1', enabled: false });
  assert.equal(ctx.$.doc('iot_rulesets', 'r1').status_key, 'disabled');
  await assert.rejects(run(ctx, T('ruleset.toggle'), { ruleset_id: 'r9', enabled: true }), { code: 'not_found', message: 'ruleset not found: r9' });
  await assert.rejects(run(ctx, T('ruleset.toggle'), { ruleset_id: 'r1' }), { code: 'invalid', message: 'ctox.iot.ruleset.toggle requires enabled' });
});

test('dashboard: Upsert, Löschen nimmt Widgets mit; fremder Realm', async () => {
  const ctx = createCtx({ seed: { iot_dashboards: [{ id: 'dX', realm: 'other', name: 'n', updated_at_ms: 1 }] } });
  const out = await run(ctx, T('dashboard.upsert'), { id: 'd1', realm: 'm', name: 'Halle', sort_index: 2 });
  assert.deepEqual(out.dashboard, { id: 'd1', realm: 'master', name: 'Halle' });
  await run(ctx, T('widget.upsert'), { id: 'w1', dashboard_id: 'd1', realm: 'm', signal_ref: 'a1::temp' });
  const del = await run(ctx, T('dashboard.delete'), { dashboard_id: 'd1' });
  assert.equal(del.deleted, 'd1');
  assert.equal(ctx.$.doc('iot_widgets', 'w1'), null);
  await assert.rejects(run(ctx, T('dashboard.upsert'), { id: 'dX', realm: 'm', name: 'n' }), { code: 'not_found', message: 'dashboard not found: dX' });
  await assert.rejects(run(ctx, T('dashboard.delete'), { dashboard_id: 'dX' }), { code: 'not_found', message: 'dashboard not found: dX' });
});

const WATCHER = JSON.stringify({ when: { op: 'gt', left: { fn: 'last' }, right: 30 }, reason: 'Serverraum zu heiß' });

test('widget.upsert: Selbstreparatur-Tor armed/needs_attention, Status bleibt ohne Code', async () => {
  const ctx = createCtx();
  await run(ctx, T('widget.upsert'), { id: 'w1', dashboard_id: 'd1', realm: 'm', signal_ref: 'a1::temp', trigger_code: WATCHER, sort_index: 3 });
  let w = ctx.$.doc('iot_widgets', 'w1');
  assert.equal(w.trigger_status, 'armed');
  assert.equal(w.sort_key, '00000003');
  await run(ctx, T('widget.upsert'), { id: 'w1', dashboard_id: 'd1', realm: 'm', signal_ref: 'a1::temp', trigger_code: 'if signal.last( {{{' });
  assert.equal(ctx.$.doc('iot_widgets', 'w1').trigger_status, 'needs_attention');
  await run(ctx, T('widget.upsert'), { id: 'w1', dashboard_id: 'd1', realm: 'm', signal_ref: 'a1::temp', cond_text: 'Zu Heiß' });
  w = ctx.$.doc('iot_widgets', 'w1');
  assert.equal(w.trigger_status, 'needs_attention');
  assert.equal(w.trigger_code, undefined); // ON CONFLICT überschreibt trigger_code mit NULL
  assert.equal(w.index_text, 'zu heiß');
  assert.equal(w.sort_key, '00000000');
});

test('widget.arrange/pause/delete: Pflichtfelder, Vorgabe paused=true, Fortsetzen prüft Wächter neu', async () => {
  const ctx = createCtx();
  await assert.rejects(run(ctx, T('widget.arrange'), { widget_id: 'w', x: 1, y: 1, w: 1.5, h: 1 }), { code: 'invalid', message: 'ctox.iot.widget.arrange requires w' });
  await assert.rejects(run(ctx, T('widget.arrange'), { widget_id: 'w', x: 1, y: 1, w: 1, h: 1 }), { code: 'not_found', message: 'widget vanished: w' });
  await run(ctx, T('widget.upsert'), { id: 'w1', dashboard_id: 'd1', realm: 'm', signal_ref: 'a1::temp', trigger_code: WATCHER });
  const arr = await run(ctx, T('widget.arrange'), { widget_id: 'w1', x: 0, y: 1, w: 4, h: 3 });
  assert.deepEqual(arr.widget, { id: 'w1', x: 0, y: 1, w: 4, h: 3 });
  assert.equal(ctx.$.doc('iot_widgets', 'w1').trigger_code, WATCHER);
  let out = await run(ctx, T('widget.pause'), { widget_id: 'w1' });
  assert.equal(out.widget.trigger_status, 'paused');
  out = await run(ctx, T('widget.pause'), { widget_id: 'w1', paused: false });
  assert.equal(out.widget.trigger_status, 'armed');
  await assert.rejects(run(ctx, T('widget.pause'), { widget_id: 'nope' }), { code: 'not_found', message: 'widget not found: nope' });
  await run(ctx, T('widget.delete'), { widget_id: 'w1' });
  assert.equal(ctx.$.doc('iot_widgets', 'w1'), null);
  out = await run(ctx, T('widget.delete'), { widget_id: 'w1' }); // fehlend: Rust löscht 0 Zeilen, kein Fehler
  assert.equal(out.deleted, 'w1');
});

test('compile_trigger/generate_render: Codegen über model, Wächter wird geprüft', async () => {
  const model = ({ metadata }) => ({ output_text: metadata.kind === 'iot_trigger_code' ? `\`\`\`json\n${WATCHER}\n\`\`\`` : 'host.textContent = api.signal.last();' });
  const ctx = createCtx({ model });
  await run(ctx, T('widget.upsert'), { id: 'w1', dashboard_id: 'd1', realm: 'm', signal_ref: 'a1::temp' });
  await assert.rejects(run(ctx, T('widget.compile_trigger'), { widget_id: 'w1' }), { code: 'invalid', message: 'widget w1 has no condition (Wenn) to compile' });
  await run(ctx, T('widget.upsert'), { id: 'w1', dashboard_id: 'd1', realm: 'm', signal_ref: 'a1::temp', cond_text: 'über 30 Grad' });
  const out = await run(ctx, T('widget.compile_trigger'), { widget_id: 'w1' });
  assert.equal(out.trigger_status, 'armed');
  assert.equal(ctx.$.doc('iot_widgets', 'w1').trigger_code, WATCHER);
  assert.equal(ctx.$.capsTo('model.run').length, 0, 'CAP-3b: model.run ist kein ctx.cap-Primitiv');
  assert.match(ctx.$.modelCalls[0].input, /Bedingung \(Wenn, Freitext\): "über 30 Grad"/);
  await run(ctx, T('widget.generate_render'), { widget_id: 'w1' });
  assert.equal(ctx.$.doc('iot_widgets', 'w1').render_code, 'host.textContent = api.signal.last();');
  await assert.rejects(run(ctx, T('widget.generate_render'), { widget_id: 'w9' }), { code: 'not_found', message: 'widget not found: w9' });
  const bad = createCtx({ model: { output_text: 'loop {}' } });
  await run(bad, T('widget.upsert'), { id: 'w1', dashboard_id: 'd1', realm: 'm', signal_ref: 'a1::temp', cond_text: 'x' });
  assert.equal((await run(bad, T('widget.compile_trigger'), { widget_id: 'w1' })).trigger_status, 'needs_attention');
});

test('agent.configure: Status „unconfigured“, link_state bleibt, Endpunkt bei net.ingest angemeldet', async () => {
  const ctx = createCtx();
  const out = await run(ctx, T('agent.configure'), { id: 'g1', realm: 'x', name: 'Broker', kind: 'mqtt', data: { host: 'h' } });
  assert.deepEqual(out.agent, { id: 'g1', realm: 'master', name: 'Broker', kind: 'mqtt', enabled: true });
  assert.equal(ctx.$.doc('iot_agent_status', 'g1').link_state, 'unconfigured');
  assert.deepEqual(ctx.$.capsTo('net.ingest.subscribe')[0].args[0], { endpoint: 'g1', protocol: 'mqtt', enabled: true, config: { host: 'h' } });
  const seeded = createCtx({ seed: { iot_agent_status: [{ id: 'g1', realm: 'master', agent_id: 'g1', link_state: 'connected', data: {}, updated_at_ms: 1 }] } });
  await run(seeded, T('agent.configure'), { id: 'g1', realm: 'x', name: 'Broker', kind: 'mqtt' });
  assert.equal(seeded.$.doc('iot_agent_status', 'g1').link_state, 'connected');
});

test('attribute.write: Koerzierung, Zeitnormierung, Outdated-Regel (§2A.1-3), Datenpunkt über knowledge.query', async () => {
  const kn = knowledgeSim();
  const ctx = createCtx({ caps: { 'knowledge.query': kn.query } });
  await run(ctx, T('asset.upsert'), asset({ id: 'a1', asset_type_info: thermostat() }));
  let out = await run(ctx, T('attribute.write'), { asset_id: 'a1', name: 'temp', value: '21.5', timestamp_ms: 0 });
  assert.equal(out.outcome, 'Updated');
  assert.equal(out.attribute.value, 21.5);
  assert.equal(out.attribute.timestamp, 1_700_000_000_000);
  out = await run(ctx, T('attribute.write'), { asset_id: 'a1', name: 'temp', value: 30, timestamp_ms: 1_000 });
  assert.equal(out.outcome, 'OutdatedRecordedOnly');
  assert.equal(ctx.$.doc('iot_assets', 'a1').data.attributes.temp.value, 21.5);
  // Kernform: knowledge.query({argv}) – erster Datenpunkt legt die Tabelle an (append → describe → create → append).
  assert.deepEqual(kn.calls.map((a) => a[1]), ['append', 'describe', 'create', 'append', 'append']);
  assert.deepEqual(kn.tables.get('iot/iot_timeseries'), [
    { asset_id: 'a1', attribute_name: 'temp', timestamp_ms: 1_700_000_000_000, value_json: '21.5' },
    { asset_id: 'a1', attribute_name: 'temp', timestamp_ms: 1_000, value_json: '30' }, // veraltet, trotzdem aufgezeichnet
  ]);
  assert.equal(ctx.$.capsTo('dataframe.table.import').length + ctx.$.capsTo('dataframe.query').length, 0);
  await assert.rejects(run(ctx, T('attribute.write'), { asset_id: 'a1', name: 'temp', value: 'heiß' }), { code: 'invalid', message: "cannot coerce 'heiß' to Number" });
  await assert.rejects(run(ctx, T('attribute.write'), { asset_id: 'zz', name: 'temp', value: 1 }), { code: 'not_found', message: 'asset not found: zz' });
  out = await run(ctx, T('attribute.write'), { asset_id: 'a1', name: 'note', value: 'hi' });
  assert.equal(out.attribute.value_type, 'Text');
});

test('datapoints.query: Formprüfung, Fensterzeile; webhook.register über net.ingest', async () => {
  const kn = knowledgeSim();
  const ctx = createCtx({ caps: { 'knowledge.query': kn.query, 'net.ingest.webhook.register': { ingest_url: 'https://t.example/in/iot/webhook' } } });
  await run(ctx, T('asset.upsert'), asset({ id: 'a1' }));
  await run(ctx, T('attribute.write'), { asset_id: 'a1', name: 'temp', value: 1, timestamp_ms: 5 });
  await run(ctx, T('attribute.write'), { asset_id: 'a1', name: 'temp', value: 9, timestamp_ms: 11 }); // außerhalb
  const base = { asset_id: 'a1', attribute_name: 'temp', from_ms: 0, to_ms: 10 };
  await assert.rejects(run(ctx, T('datapoints.query'), { ...base, shape: 'interval' }), { message: 'interval shape requires interval_ms' });
  await assert.rejects(run(ctx, T('datapoints.query'), { ...base, shape: 'x' }), { message: 'unknown datapoints shape: x (expected all|interval|lttb)' });
  const out = await run(ctx, T('datapoints.query'), { ...base, shape: 'ALL' });
  assert.equal(out.window, 'a1:temp:0:10:all');
  assert.equal(ctx.$.doc('iot_datapoints', 'a1:temp:0:10:all').point_count, 1);
  const reg = await run(ctx, T('webhook.register'), { realm: 'evil', signal_ref: 'a1::temp' });
  // §16.38 Nr. 1 + Ergänzung: nur {path, command, pass_headers, default_response, aliases} – kein Realm, keine Zustellform (tests/webhook.test.mjs)
  assert.deepEqual(Object.keys(ctx.$.capsTo('net.ingest.webhook.register')[0].args[0]).sort(), ['aliases', 'command', 'default_response', 'pass_headers', 'path']);
  assert.equal(ctx.$.doc('iot_agents', `webhook:${reg.id}`).realm, 'master');
});

const trigger = (doc) => ({ type: 'trigger', trigger_id: 't1', change: { collection: 'iot_attributes', op: 'update', doc }, actor: { id: 'system' } });
async function evaluate(ctx, assetId, name) {
  const { default: handle } = await import('../ctox.iot.signal.evaluate.js');
  return handle(trigger(ctx.$.doc('iot_attributes', `${assetId}:${name}`)), ctx);
}

test('signal.evaluate: Regel feuert beim Übergang, Alarm wie emit_matches, kein Doppelfeuer', async () => {
  const ctx = createCtx();
  await run(ctx, T('asset.upsert'), asset({ id: 'a1', asset_type_info: thermostat() }));
  await run(ctx, T('ruleset.save'), { id: 'r1', realm: 'm', name: 'Hitze', data: {
    name: 'Hitze-Regel', when: { items: [{ attribute: { name: 'temp', value: { op: 'greater_than', value: 30 } } }] }, on_match: { alarm_severity: 'high' },
  } });
  await run(ctx, T('attribute.write'), { asset_id: 'a1', name: 'temp', value: 35 });
  let rep = await evaluate(ctx, 'a1', 'temp');
  assert.deepEqual(rep.rulesets, ['r1']);
  const alarm = ctx.$.all('iot_alarms')[0];
  assert.equal(alarm.title, 'IoT: Hitze-Regel on a1');
  assert.equal(alarm.severity, 'HIGH');
  assert.equal(alarm.status, 'OPEN');
  assert.equal(alarm.data.content, 'ruleset=Hitze attribute=temp matched');
  assert.equal(alarm.source, 'RealmRuleset');
  rep = await evaluate(ctx, 'a1', 'temp');
  assert.deepEqual(rep.rulesets, []);
  await run(ctx, T('attribute.write'), { asset_id: 'a1', name: 'temp', value: 20 });
  await evaluate(ctx, 'a1', 'temp');
  await run(ctx, T('attribute.write'), { asset_id: 'a1', name: 'temp', value: 31 });
  rep = await evaluate(ctx, 'a1', 'temp');
  assert.deepEqual(rep.rulesets, ['r1']);
  await run(ctx, T('ruleset.toggle'), { ruleset_id: 'r1', enabled: false });
  await run(ctx, T('attribute.write'), { asset_id: 'a1', name: 'temp', value: 20 });
  await run(ctx, T('attribute.write'), { asset_id: 'a1', name: 'temp', value: 40 });
  rep = await evaluate(ctx, 'a1', 'temp');
  assert.deepEqual(rep.rulesets, []);
});

test('signal.evaluate: Wiederholsperre recurrence.mins (conditions.rs:302-342)', async () => {
  const ctx = createCtx();
  await run(ctx, T('asset.upsert'), asset({ id: 'a1', asset_type_info: thermostat() }));
  await run(ctx, T('ruleset.save'), { id: 'r1', realm: 'm', name: 'H', data: {
    name: 'H', recurrence: { mins: 5 }, when: { items: [{ attribute: { name: 'temp', value: { op: 'greater_equal', value: 30 } } }] },
  } });
  await run(ctx, T('attribute.write'), { asset_id: 'a1', name: 'temp', value: 30 });
  assert.deepEqual((await evaluate(ctx, 'a1', 'temp')).rulesets, ['r1']);
  ctx.$.setNow(1_700_000_000_000 + 60_000);
  assert.deepEqual((await evaluate(ctx, 'a1', 'temp')).rulesets, []);
  ctx.$.setNow(1_700_000_000_000 + 5 * 60_000);
  assert.deepEqual((await evaluate(ctx, 'a1', 'temp')).rulesets, ['r1']);
});

test('signal.evaluate: Widget-Wächter feuert → Alarm + agent.task; kaputter Wächter → needs_attention', async () => {
  const now = 1_700_000_000_000;
  const kn = knowledgeSim();
  const ctx = createCtx({ caps: { 'knowledge.query': kn.query } });
  await run(ctx, T('asset.upsert'), asset({ id: 'a1', asset_type_info: thermostat() }));
  await run(ctx, T('attribute.write'), { asset_id: 'a1', name: 'temp', value: 22, timestamp_ms: now - 2000 });
  await run(ctx, T('widget.upsert'), { id: 'w1', dashboard_id: 'd1', realm: 'm', signal_ref: 'a1::temp', trigger_code: WATCHER, action_prompt: 'Lüftung an' });
  await run(ctx, T('widget.upsert'), { id: 'w2', dashboard_id: 'd1', realm: 'm', signal_ref: 'a1::temp', trigger_code: WATCHER });
  await run(ctx, T('widget.pause'), { widget_id: 'w2' });
  await run(ctx, T('attribute.write'), { asset_id: 'a1', name: 'temp', value: 35 });
  const rep = await evaluate(ctx, 'a1', 'temp');
  assert.deepEqual(rep.widgets, ['w1']);
  assert.equal(ctx.$.doc('iot_widgets', 'w1').trigger_status, 'fired');
  assert.equal(ctx.$.doc('iot_widgets', 'w2').trigger_status, 'paused');
  const alarm = ctx.$.all('iot_alarms')[0];
  assert.equal(alarm.title, 'IoT-Auftrag ausgelöst: a1::temp');
  assert.equal(alarm.data.content, 'Serverraum zu heiß');
  const task = ctx.$.capsTo('agent.task.enqueue')[0].args[0];
  assert.equal(task.thread_key, 'iot-widget:w1');
  assert.match(task.instruction, /^Lüftung an\n\n— Auslöser —\nSignal: a1::temp\nAsset: a1\nGrund: Serverraum zu heiß\nLetztwert: 35 \(vor 0s\)\nLetzte Werte: 22, 35/);
  // Wächter, der importiert wurde (Rhai aus dem Core) → needs_attention statt Abbruch
  const legacy = createCtx({ seed: { iot_widgets: [{ id: 'w3', dashboard_id: 'd', realm: 'master', signal_ref: 'a1::temp', trigger_code: 'if signal.last() > 30.0 { fire("x"); }', trigger_status: 'armed', updated_at_ms: 1 }] } });
  await run(legacy, T('asset.upsert'), asset({ id: 'a1', asset_type_info: thermostat() }));
  await run(legacy, T('attribute.write'), { asset_id: 'a1', name: 'temp', value: 35 });
  await evaluate(legacy, 'a1', 'temp');
  assert.equal(legacy.$.doc('iot_widgets', 'w3').trigger_status, 'needs_attention');
});

test('Wächter: validate/evaluate wie validate_program-Fälle (watcher.rs:538-547), for_ms = „seit X“', () => {
  assert.equal(validateWatcher(WATCHER), null);
  assert.notEqual(validateWatcher('   '), null);
  assert.notEqual(validateWatcher('if signal.last( {{{'), null);
  assert.notEqual(validateWatcher(JSON.stringify({ when: { op: 'gt', left: { fn: 'avg', window: '15x' }, right: 1 } })), null);
  const pts = [[1, 22], [2, 35]];
  assert.deepEqual(evaluateWatcher(WATCHER, pts, 100, {}).fired, ['Serverraum zu heiß']);
  assert.deepEqual(evaluateWatcher(WATCHER, [[1, 22], [2, 24]], 100, {}).fired, []);
  const slow = JSON.stringify({ when: { op: 'gt', left: { fn: 'last' }, right: 30 }, reason: 'heiß seit 1 min', for_ms: 60_000 });
  let r = evaluateWatcher(slow, pts, 0, {});
  assert.deepEqual(r.fired, []);
  r = evaluateWatcher(slow, pts, 60_000, r.state);
  assert.deepEqual(r.fired, ['heiß seit 1 min']);
  const avg = JSON.stringify({ when: { op: 'gte', left: { fn: 'avg', window: '1s' }, right: 30 } });
  assert.equal(evaluateWatcher(avg, [[0, 10], [9_500, 30], [10_000, 32]], 10_000, {}).fired.length, 1);
});

test('kein Akteur → forbidden', async () => {
  const { default: handle } = await import('../ctox.iot.dashboard.upsert.js');
  const cmd = makeCommand(T('dashboard.upsert'), { realm: 'm', name: 'n' });
  delete cmd.actor;
  await assert.rejects(handle(cmd, createCtx()), { code: 'forbidden', message: 'iot: forbidden' });
});

test('datapoints.query: Formen all/interval/lttb in der App (datapoints.rs:120-495), Zeitreihe über knowledge.query', async () => {
  const kn = knowledgeSim();
  const ctx = createCtx({ caps: { 'knowledge.query': kn.query } });
  await run(ctx, T('asset.upsert'), asset({ id: '42' })); // zahlartige id: Gleichheit als String (~ + exakter Nachfilter)
  await run(ctx, T('asset.upsert'), asset({ id: '420' }));
  for (const [ts, v] of [[3, 1], [12, 2], [14, 3], [35, 4]]) {
    await run(ctx, T('attribute.write'), { asset_id: '42', name: 'p', value: v, timestamp_ms: ts });
  }
  await run(ctx, T('attribute.write'), { asset_id: '420', name: 'p', value: 99, timestamp_ms: 13 });
  const q = (extra) => run(ctx, T('datapoints.query'), { asset_id: '42', attribute_name: 'p', from_ms: 10, to_ms: 39, ...extra });
  const all = await q({ shape: 'all' });
  assert.deepEqual(all.points, [{ timestamp: 12, value: 2 }, { timestamp: 14, value: 3 }, { timestamp: 35, value: 4 }]);
  assert.equal(all.truncated, false);
  // interval: Bucket-Ausrichtung ab Epoche, letzter Wert je Bucket, LOCF aus dem Datenpunkt vor dem ersten Bucket
  const iv = await q({ shape: 'interval', interval_ms: 10 });
  assert.deepEqual(iv.points, [{ timestamp: 10, value: 3 }, { timestamp: 20, value: 3 }, { timestamp: 30, value: 4 }]);
  const lead = await run(ctx, T('datapoints.query'), { asset_id: '42', attribute_name: 'p', from_ms: 0, to_ms: 9, shape: 'interval', interval_ms: 1 });
  assert.equal(lead.points[0].value, null); // führende Lücke ohne Vorwert
  assert.equal(lead.points[3].value, 1);
  // lttb: erster und letzter Punkt bleiben; Schwelle >= n → unverändert
  const lt = await q({ shape: 'lttb', threshold: 2 });
  assert.deepEqual(lt.points.map((p) => p.timestamp), [12, 14, 35]);
  const lt3 = await run(ctx, T('datapoints.query'), { asset_id: '42', attribute_name: 'p', from_ms: 0, to_ms: 99, shape: 'lttb', threshold: 3 });
  assert.deepEqual(lt3.points.map((p) => p.timestamp), [3, 14, 35]);
  kn.tables.get('iot/iot_timeseries').push({ asset_id: '42', attribute_name: 'p', timestamp_ms: 36, value_json: '"x"' });
  await assert.rejects(q({ shape: 'lttb', threshold: 3 }), { code: 'invalid', message: /LTTB requires numeric\/boolean values; non-numeric sample at ts=36/ });
  assert.equal(ctx.$.capsTo('dataframe.query').length, 0);
});
