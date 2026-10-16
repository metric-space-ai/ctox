// Strang SKL-A4: IoT-Alarm-Aufträge im Zielbild §16.30 – Skill iot/operations aus dem App-Paket, Rückrufe
// ctox.iot.task.complete/.failed an den Alarm. Simulation v4/tools/agent-sim (Enqueue-Prüfung, Skill-Auflösung gegen
// das Paket, genau eine Zustellung je task_id).
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { createCtx, run } from './_ctx.mjs';
import { knowledgeSim } from './_knowledge-sim.mjs';
import { createAgentTaskSim } from '../../../../v4/tools/agent-sim/agent-task-sim.mjs';

const APP_DIR = fileURLToPath(new URL('../..', import.meta.url));
const T = (op) => `ctox.iot.${op}`;
const N = 1_700_000_000_000;
const WATCHER = JSON.stringify({ when: { op: 'gt', left: { fn: 'last' }, right: 30 }, reason: 'Serverraum zu heiß' });
const typeInfo = { asset_type: 'Thermostat', attributes: [{ name: 'temp', value_descriptor: { name: 'number', base_type: 'Number' }, meta: {} }] };

// Ein gemeinsamer Speicher-ctx; der Sim hängt agent.task und asset an (cap-Proxy des Test-ctx umschlossen).
function simCtx(ctx) {
  const root = ctx.cap;
  const agent = {};
  ctx.cap = new Proxy({}, { get: (_t, name) => (name === 'agent' ? agent : root[name]) });
  return ctx;
}

async function setup({ rule = true, widget = false, onMatch = {} } = {}) {
  const kn = knowledgeSim();
  const ctx = simCtx(createCtx({ now: N, caps: { 'knowledge.query': kn.query } }));
  const sim = createAgentTaskSim({ appId: 'iot', appDir: APP_DIR, ctxFactory: () => ctx });
  sim.attach(ctx, null);
  await run(ctx, T('asset.upsert'), { id: 'a1', realm: 'master', asset_type: 'Thermostat', name: 'Serverraum', asset_type_info: typeInfo });
  if (rule) {
    await run(ctx, T('ruleset.save'), { id: 'r1', realm: 'master', name: 'Hitze', data: {
      name: 'Hitze-Regel', when: { items: [{ attribute: { name: 'temp', value: { op: 'greater_than', value: 30 } } }] },
      on_match: { enqueue_task: true, task_prompt: 'Prüfe die Kühlung.', ...onMatch },
    } });
  }
  if (widget) {
    await run(ctx, T('attribute.write'), { asset_id: 'a1', name: 'temp', value: 22, timestamp_ms: N - 2000 });
    await run(ctx, T('widget.upsert'), { id: 'w1', dashboard_id: 'd1', realm: 'm', signal_ref: 'a1::temp', trigger_code: WATCHER, action_prompt: 'Lüftung an' });
  }
  await run(ctx, T('attribute.write'), { asset_id: 'a1', name: 'temp', value: 35 });
  const { default: evaluate } = await import('../ctox.iot.signal.evaluate.js');
  const doc = ctx.$.doc('iot_attributes', 'a1:temp');
  const report = await evaluate({ type: 'trigger', trigger_id: 't1', change: { collection: 'iot_attributes', op: 'update', doc }, actor: { id: 'system' } }, ctx);
  return { ctx, sim, report, kn };
}
const alarmOf = (ctx, id) => ctx.$.doc('iot_alarms', id);

test('Regeltreffer: enqueue mit Paket-Skill iot/operations, Callbacks, Idempotenzschlüssel; Prompt = Collection-Datum', async () => {
  const { sim, report } = await setup({ onMatch: { suggested_skill: 'fremd-skill' } });
  assert.equal(sim.enqueued.length, 1);
  const [{ args, skill }] = sim.enqueued;
  assert.deepEqual(skill, { skill: 'iot/operations', scope: 'app', path: 'skills/operations/SKILL.md' });
  assert.equal(args.on_complete, 'ctox.iot.task.complete');
  assert.equal(args.on_failed, 'ctox.iot.task.failed');
  assert.equal(args.instruction, 'Prüfe die Kühlung.');
  assert.equal(args.thread_key, 'iot-ruleset:r1:a1');
  assert.deepEqual(args.payload, { alarm_id: report.alarms[0], ruleset_id: 'r1', asset_id: 'a1' });
  assert.equal(args.idempotency_key, `iot-alarm:${report.alarms[0]}`);
});

test('Widget-Wächter: enqueue mit Paket-Skill, Seed-Prompt nennt iot/operations, Callbacks', async () => {
  const { ctx, sim, report } = await setup({ rule: false, widget: true });
  assert.deepEqual(report.widgets, ['w1']);
  const [{ args, skill }] = sim.enqueued;
  const alarmId = ctx.$.all('iot_alarms')[0].id;
  assert.equal(skill.path, 'skills/operations/SKILL.md');
  assert.equal(args.on_complete, 'ctox.iot.task.complete');
  assert.equal(args.on_failed, 'ctox.iot.task.failed');
  assert.match(args.instruction, /^Lüftung an\n\n— Auslöser —\nSignal: a1::temp\n[\s\S]*Handle gemäß Skill `iot\/operations`\. Die Überwachung läuft weiter\.$/);
  assert.deepEqual(args.payload, { alarm_id: alarmId, widget_id: 'w1', signal_ref: 'a1::temp' });
  assert.equal(args.idempotency_key, `iot-alarm:${alarmId}`);
});

test('complete: Urteil am Alarm, Alarm quittiert; Doppelzustellung ohne Wirkung', async () => {
  const { ctx, sim, report } = await setup();
  const alarmId = report.alarms[0];
  const [{ task_id }] = sim.enqueued;
  const reply = { diagnose: 'temp 35 °C > 30', massnahme: 'Kühlung prüfen', alarm: 'acknowledge', schreibvorschlaege: [], notizen: '' };
  const d = await sim.complete(task_id, { text: JSON.stringify(reply) });
  assert.equal(d.delivered, true);
  assert.equal(d.error, undefined, d.error?.message);
  const a = alarmOf(ctx, alarmId);
  assert.equal(a.status, 'ACKNOWLEDGED');
  assert.equal(a.status_key, 'ACKNOWLEDGED');
  assert.equal(a.data.status, 'Acknowledged');
  assert.deepEqual(a.data.agent_task, {
    task_id, status: 'completed', alarm_decision: 'acknowledge', diagnose: 'temp 35 °C > 30', massnahme: 'Kühlung prüfen',
    notizen: '', schreibvorschlaege: [], finished_ms: N,
  });
  // zweite Zustellung durch den Sim verworfen
  assert.deepEqual(await sim.complete(task_id, { text: '{}' }), { delivered: false, reason: 'duplicate' });
  // und am Handler selbst (Core stellt doch doppelt zu): keine Schreibwirkung
  const writes = ctx.$.writes.length;
  ctx.$.setNow(N + 5000);
  const { default: complete } = await import('../ctox.iot.task.complete.js');
  const again = await complete({ ...d.command, payload: { ...d.command.payload, result: { text: JSON.stringify({ alarm: 'resolve' }) } } }, ctx);
  assert.equal(again.duplicate, true);
  assert.equal(ctx.$.writes.length, writes);
  assert.equal(alarmOf(ctx, alarmId).status, 'ACKNOWLEDGED');
  // auch ein später failed-Rückruf derselben task_id ändert nichts
  const { default: failed } = await import('../ctox.iot.task.failed.js');
  assert.equal((await failed({ ...d.command, type: T('task.failed'), payload: { task_id, status: 'failed', result: null, error: { code: 'x', message: 'y' } } }, ctx)).duplicate, true);
  assert.equal(alarmOf(ctx, alarmId).data.agent_task.status, 'completed');
});

test('complete: resolve löst, keep_open lässt offen, Freitext = Diagnose + quittieren; Übergangsregeln', async () => {
  for (const [alarm, expected] of [['resolve', 'RESOLVED'], ['keep_open', 'OPEN']]) {
    const { ctx, sim, report } = await setup();
    await sim.complete(sim.enqueued[0].task_id, { text: '```json\n' + JSON.stringify({ diagnose: 'd', alarm }) + '\n```' });
    assert.equal(alarmOf(ctx, report.alarms[0]).status, expected, alarm);
    assert.equal(alarmOf(ctx, report.alarms[0]).data.agent_task.alarm_decision, alarm);
  }
  const free = await setup();
  await free.sim.complete(free.sim.enqueued[0].task_id, { text: 'Kühlung läuft wieder.' });
  const fa = alarmOf(free.ctx, free.report.alarms[0]);
  assert.equal(fa.status, 'ACKNOWLEDGED');
  assert.equal(fa.data.agent_task.diagnose, 'Kühlung läuft wieder.');
  // Alarm wurde inzwischen vom Menschen geschlossen: acknowledge/resolve ändern den Status nicht (Closed ist terminal)
  const closed = await setup();
  await run(closed.ctx, T('alarm.update'), { alarm_id: closed.report.alarms[0], action: 'close' });
  await closed.sim.complete(closed.sim.enqueued[0].task_id, { text: JSON.stringify({ alarm: 'resolve' }) });
  const ca = alarmOf(closed.ctx, closed.report.alarms[0]);
  assert.equal(ca.status, 'CLOSED');
  assert.equal(ca.data.agent_task.status, 'completed');
  // bereits gelöst: acknowledge setzt nicht zurück (Resolved → Acknowledged ist kein Übergang)
  const resolved = await setup();
  await run(resolved.ctx, T('alarm.update'), { alarm_id: resolved.report.alarms[0], action: 'resolve' });
  await resolved.sim.complete(resolved.sim.enqueued[0].task_id, { text: JSON.stringify({ alarm: 'acknowledge' }) });
  assert.equal(alarmOf(resolved.ctx, resolved.report.alarms[0]).status, 'RESOLVED');
});

test('complete: Schreibvorschläge nur am Alarm, keine Gerätschreibung', async () => {
  const { ctx, sim, report, kn } = await setup();
  const before = { attr: JSON.stringify(ctx.$.doc('iot_attributes', 'a1:temp')), points: (kn.tables.get('iot/iot_timeseries') ?? []).length };
  const writes = [{ asset_id: 'a1', attribute: 'fan', value: true, grund: 'kühlen' }, { attribute: 'ohne-asset' }, 'kaputt'];
  await sim.complete(sim.enqueued[0].task_id, { text: JSON.stringify({ diagnose: 'd', alarm: 'keep_open', schreibvorschlaege: writes }) });
  assert.deepEqual(alarmOf(ctx, report.alarms[0]).data.agent_task.schreibvorschlaege, [{ asset_id: 'a1', attribute: 'fan', value: true, grund: 'kühlen' }]);
  assert.equal(JSON.stringify(ctx.$.doc('iot_attributes', 'a1:temp')), before.attr);
  assert.equal(ctx.$.doc('iot_attributes', 'a1:fan'), null);
  assert.equal((kn.tables.get('iot/iot_timeseries') ?? []).length, before.points);
});

test('failed/cancelled: Zustand am Alarm, Alarmstatus bleibt offen; Doppelzustellung ohne Wirkung', async () => {
  const { ctx, sim } = await setup({ rule: false, widget: true });
  const [{ task_id }] = sim.enqueued;
  const widgetAlarm = ctx.$.all('iot_alarms')[0].id;
  const d = await sim.fail(task_id, { code: 'model_error', message: 'Modell nicht erreichbar' });
  assert.equal(d.delivered, true);
  assert.equal(d.command.type, 'ctox.iot.task.failed');
  const a = alarmOf(ctx, widgetAlarm);
  assert.equal(a.status, 'OPEN');
  assert.deepEqual(a.data.agent_task, { task_id, status: 'failed', error: { code: 'model_error', message: 'Modell nicht erreichbar' }, finished_ms: N });
  assert.deepEqual(await sim.fail(task_id, 'nochmal'), { delivered: false, reason: 'duplicate' });
  // Abbruch (§16.30-Festlegung: on_failed mit status 'cancelled')
  const c = await setup();
  const cid = c.sim.enqueued[0].task_id;
  const { default: failed } = await import('../ctox.iot.task.failed.js');
  await failed({ id: 'cmd_cancel', type: T('task.failed'), payload: { task_id: cid, status: 'cancelled', result: null } }, c.ctx);
  const ca = alarmOf(c.ctx, c.report.alarms[0]);
  assert.equal(ca.status, 'OPEN');
  assert.equal(ca.data.agent_task.status, 'cancelled');
  const writes = c.ctx.$.writes.length;
  const dup = await failed({ id: 'cmd_cancel2', type: T('task.failed'), payload: { task_id: cid, status: 'cancelled', result: null } }, c.ctx);
  assert.equal(dup.duplicate, true);
  assert.equal(c.ctx.$.writes.length, writes);
});

test('Rückruf ohne task_id → invalid; gelöschter Alarm → ohne Wirkung', async () => {
  const { ctx, sim, report } = await setup();
  const { default: complete } = await import('../ctox.iot.task.complete.js');
  await assert.rejects(complete({ type: T('task.complete'), payload: { status: 'completed' } }, ctx), { code: 'invalid' });
  await ctx.remove('iot_alarms', report.alarms[0]);
  const writes = ctx.$.writes.length;
  const d = await sim.complete(sim.enqueued[0].task_id, { text: '{}' });
  assert.equal(d.result.skipped, 'alarm_missing');
  assert.equal(ctx.$.writes.length, writes);
});

test('Paket: skills/operations/SKILL.md (Name = Ordner), ohne CLI-/MCP-Rückschreibwege', () => {
  const skill = readFileSync(new URL('../../skills/operations/SKILL.md', import.meta.url), 'utf8');
  assert.match(skill, /^---\nname: operations\ndescription: .+\n---\n/);
  assert.doesNotMatch(skill, /ctox iot |ctox\.iot\.[a-z_.]+ --|business_os\.(put|update|command)/);
  assert.match(skill, /ctox\.iot\.task\.complete/);
});
