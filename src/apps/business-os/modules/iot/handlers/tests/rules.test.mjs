// Regelbestandteile Dauerfenster, timestamp_older_than, Meta-Prädikate, track_unmatched (Strang IOT2) gegen die
// Rust-Referenz src/core/iot/conditions.rs. Rust-Tests sind übernommen (Name + Zeile im Testtitel), Zeiten auf die
// Test-Uhr N verschoben; Fälle ohne Rust-Test sind aus dem Code (Zeilenanker) abgeleitet.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run } from './_ctx.mjs';
import { attributeMatches, evaluateCondition, COND, ruleIsWellFormed } from '../_iotcore.js';

const N = 1_700_000_000_000;
const T = (op) => `ctox.iot.${op}`;
const typeInfo = (meta = { unit: 'celsius' }) => ({
  asset_type: 'Thing',
  attributes: ['temp', 'humidity'].map((name) => ({ name, value_descriptor: { name: 'number', base_type: 'Number' }, meta })),
});
const gt = (attr, threshold, extra = {}) => ({ attribute: { name: attr, value: { op: 'greater_than', value: threshold } }, ...extra });
const rule = (when, extra = {}) => ({ name: 'breach', when: { operator: 'And', items: when }, ...extra });

async function setup(data, { meta, id = 'rs' } = {}) {
  const ctx = createCtx({ now: N });
  await run(ctx, T('asset.upsert'), { id: 'asset-1', realm: 'master', asset_type: 'Thing', name: 'asset-1', asset_type_info: typeInfo(meta) });
  await run(ctx, T('ruleset.save'), { id, realm: 'master', name: id, data });
  return ctx;
}
// write_event + evaluate_event der Rust-Tests: Datenpunkt schreiben (ts = jetzt) und den on-Trigger auswerten.
async function writeEval(ctx, name, value, at) {
  ctx.$.setNow(at);
  await run(ctx, T('attribute.write'), { asset_id: 'asset-1', name, value, timestamp_ms: at });
  const { default: handle } = await import('../ctox.iot.signal.evaluate.js');
  const doc = ctx.$.doc('iot_attributes', `asset-1:${name}`);
  return handle({ type: 'trigger', trigger_id: 'on', change: { collection: 'iot_attributes', op: 'update', doc }, actor: { id: 'system' } }, ctx);
}
async function tick(ctx, at) {
  ctx.$.setNow(at);
  const { default: handle } = await import('../ctox.iot.rules.tick.js');
  return handle({ type: 'trigger', trigger_id: 'every', actor: { id: 'system' } }, ctx);
}
const state = (ctx, id = 'rs') => ctx.$.doc('iot_rulesets', id).match_state;

// ---------------------------------------------------------------- Meta-Prädikate (§2A.23)

test('meta: meta_item_anymatch_and_anded (conditions.rs:1320) – UND, je anyMatch; fehlender Schlüssel scheitert', () => {
  const meta = { severity: 'critical', rules: true };
  const pred = { name: 'temp', meta: [{ name: 'severity', value: { op: 'eq', value: 'critical' } }, { name: 'rules' }] };
  assert.equal(attributeMatches(pred, 1, 0, meta, null, 1000), true);
  assert.equal(attributeMatches({ name: 'temp', meta: [{ name: 'absent' }] }, 1, 0, meta, null, 1000), false);
  // Wertprädikat auf dem Meta-Wert muss halten (conditions.rs:518-521)
  assert.equal(attributeMatches({ name: 'temp', meta: [{ name: 'severity', value: { op: 'eq', value: 'low' } }] }, 1, 0, meta, null, 1000), false);
  // UND: ein erfülltes + ein unerfülltes Meta-Prädikat → falsch
  assert.equal(attributeMatches({ name: 'temp', meta: [{ name: 'rules' }, { name: 'absent' }] }, 1, 0, meta, null, 1000), false);
});

test('meta: Regel feuert nur, wenn die Attribut-Meta passt (Ende-zu-Ende über signal.evaluate)', async () => {
  const when = [{ attribute: { name: 'temp', value: { op: 'greater_than', value: 50 }, meta: [{ name: 'unit', value: { op: 'eq', value: 'celsius' } }] } }];
  let ctx = await setup(rule(when));
  assert.deepEqual((await writeEval(ctx, 'temp', 60, N)).rulesets, ['rs']);
  ctx = await setup(rule(when), { meta: { unit: 'kelvin' } });
  assert.deepEqual((await writeEval(ctx, 'temp', 60, N)).rulesets, []);
  assert.equal(ctx.$.all('iot_alarms').length, 0);
});

// ---------------------------------------------------------------- timestamp_older_than (§2A.16)

test('timestamp_older_than: erfüllt genau ab attr.timestamp <= now - dur (conditions.rs:492-499), fehlendes Attribut = 0', () => {
  const pred = { name: 'temp', timestamp_older_than: 60_000 };
  assert.equal(attributeMatches(pred, 1, N - 60_000, {}, null, N), true, 'Grenze: ts == now - dur hält');
  assert.equal(attributeMatches(pred, 1, N - 60_001, {}, null, N), true);
  assert.equal(attributeMatches(pred, 1, N - 59_999, {}, null, N), false, 'ts > now - dur hält nicht');
  const asset = { id: 'a', asset_type: 'T', realm: 'master', attributes: {} };
  assert.equal(evaluateCondition({ attribute: pred }, '0', asset, { attribute_name: null }, N, {}), COND.MATCHED, 'fehlendes Attribut → Zeitstempel 0');
});

test('timestamp_older_than: every-Trigger erkennt verstummtes Attribut ohne neues Signal; neues Signal setzt zurück', async () => {
  const ctx = await setup(rule([{ attribute: { name: 'temp', timestamp_older_than: 10 * 60_000 } }]));
  assert.deepEqual((await writeEval(ctx, 'temp', 20, N)).rulesets, [], 'frischer Wert ist nicht veraltet');
  assert.deepEqual((await tick(ctx, N + 10 * 60_000 - 1)).rulesets, []);
  const rep = await tick(ctx, N + 10 * 60_000);
  assert.deepEqual(rep.rulesets, ['rs'], 'nach 10 min ohne Signal feuert die Regel');
  const alarm = ctx.$.all('iot_alarms')[0];
  assert.equal(alarm.title, 'IoT: breach on asset-1');
  assert.equal(alarm.data.content, 'ruleset=rs attribute=temp matched');
  assert.deepEqual((await tick(ctx, N + 11 * 60_000)).rulesets, [], 'kein Doppelfeuer');
  assert.deepEqual((await writeEval(ctx, 'temp', 21, N + 12 * 60_000)).rulesets, []);
  assert.equal(state(ctx)['asset-1'].matched, false, 'neues Signal re-armiert');
  assert.deepEqual((await tick(ctx, N + 22 * 60_000)).rulesets, ['rs']);
});

// ---------------------------------------------------------------- Dauerfenster (§2A.16)

test('duration: duration_window_reset_on_predicate_false_and_restart (conditions.rs:1410)', async () => {
  const ctx = await setup(rule([gt('temp', 50, { duration: 5_000 })]));
  // t0: Prädikat wahr → Fenster startet, noch kein Treffer
  assert.deepEqual((await writeEval(ctx, 'temp', 60, N + 1_000)).rulesets, [], 'duration window started, should not match yet');
  assert.equal(state(ctx)['asset-1'].windows['0'], N + 1_000);
  // t0+5s, Prädikat weiter wahr → Treffer (Grenze inklusive: first + dur <= now)
  assert.deepEqual((await writeEval(ctx, 'temp', 61, N + 6_000)).rulesets, ['rs'], 'window elapsed with predicate held → match');
  // Prädikat falsch → first_match_ms gelöscht
  await writeEval(ctx, 'temp', 10, N + 7_000);
  assert.equal(state(ctx)['asset-1'].windows, undefined, 'predicate-false must DELETE first_match_ms');
  // wieder wahr bei 8 s: Fenster startet neu, kein Sofort-Treffer
  assert.deepEqual((await writeEval(ctx, 'temp', 70, N + 8_000)).rulesets, [], 'timer restarts on false→true, not resumed');
  assert.equal(state(ctx)['asset-1'].windows['0'], N + 8_000, 'window restarts at the new now');
});

test('duration: duration_none_matches_immediately (conditions.rs:1495)', async () => {
  const ctx = await setup(rule([gt('temp', 50)]));
  assert.deepEqual((await writeEval(ctx, 'temp', 60, N + 1_000)).rulesets, ['rs'], 'no duration → immediate match');
});

test('duration: Ablauf ohne neues Signal über den every-Trigger; nur Assets mit offenem Fenster', async () => {
  const ctx = await setup(rule([gt('temp', 50, { duration: 5 * 60_000 })]));
  assert.equal((await tick(ctx, N)).evaluated, 0, 'ohne Ereignis kein Fenster, keine Auswertung');
  await writeEval(ctx, 'temp', 60, N);
  let rep = await tick(ctx, N + 5 * 60_000 - 1);
  assert.equal(rep.evaluated, 1);
  assert.deepEqual(rep.rulesets, [], 'Fenster noch offen');
  rep = await tick(ctx, N + 5 * 60_000);
  assert.deepEqual(rep.rulesets, ['rs'], 'Fenster abgelaufen, Prädikat hielt → Treffer ohne neues Signal');
  assert.equal(ctx.$.all('iot_alarms').length, 1);
  assert.deepEqual((await tick(ctx, N + 6 * 60_000)).rulesets, [], 'kein Doppelfeuer');
});

test('duration: Ereignis an einem anderen Attribut des Assets lässt das Fenster ablaufen (Rust: jede Regel je Ereignis, :716-745)', async () => {
  const ctx = await setup(rule([gt('temp', 50, { duration: 5_000 })]));
  await writeEval(ctx, 'temp', 60, N);
  assert.deepEqual((await writeEval(ctx, 'humidity', 10, N + 4_999)).rulesets, []);
  assert.deepEqual((await writeEval(ctx, 'humidity', 11, N + 5_000)).rulesets, ['rs']);
});

test('duration: AND aus Dauerfenster + Sofortbedingung (stale-AND, :739-761)', async () => {
  const ctx = await setup(rule([gt('temp', 50, { duration: 5_000 }), gt('humidity', 80)]));
  await writeEval(ctx, 'humidity', 90, N);
  assert.deepEqual((await writeEval(ctx, 'temp', 60, N + 1_000)).rulesets, [], 'Fenster offen');
  assert.deepEqual((await writeEval(ctx, 'humidity', 10, N + 6_000)).rulesets, [], 'Sofortbedingung fällt');
  assert.deepEqual((await writeEval(ctx, 'humidity', 95, N + 7_000)).rulesets, ['rs'], 'Fenster lief weiter (Prädikat temp blieb wahr)');
});

// ---------------------------------------------------------------- track_unmatched (§2A.18)

test('track_unmatched: otherwise_only_with_track_unmatched_and_filtered (conditions.rs:1516)', async () => {
  const ctx = await setup(rule([gt('temp', 50)], { track_unmatched: true }));
  let rep = await writeEval(ctx, 'temp', 10, N + 1_000);
  assert.deepEqual(rep.otherwise, ['rs']);
  assert.deepEqual(rep.rulesets, []);
  assert.equal(ctx.$.all('iot_alarms').length, 0, 'OtherwiseUnmatched löst keinen Alarm aus (emit_matches :964-968)');
  rep = await writeEval(ctx, 'temp', 11, N + 2_000);
  assert.deepEqual(rep.otherwise, [], 'previously_unmatched filters repeat otherwise');
  // Wiedertreffer setzt den Filter zurück (conditions.rs:892-903) → nächstes Unmatched erscheint wieder
  assert.deepEqual((await writeEval(ctx, 'temp', 60, N + 3_000)).rulesets, ['rs']);
  assert.equal(state(ctx)['asset-1'].unmatched, undefined);
  assert.deepEqual((await writeEval(ctx, 'temp', 12, N + 4_000)).otherwise, ['rs']);
});

test('track_unmatched: or_mode_does_not_collect_unmatched (conditions.rs:1554); ohne Schalter kein otherwise; Pending zählt nicht', async () => {
  let ctx = await setup({ ...rule([gt('temp', 50)], { track_unmatched: true }), when: { operator: 'Or', items: [gt('temp', 50)] } });
  assert.deepEqual((await writeEval(ctx, 'temp', 10, N + 1_000)).otherwise, [], 'OR mode does not collect unmatched');
  ctx = await setup(rule([gt('temp', 50)]));
  assert.deepEqual((await writeEval(ctx, 'temp', 10, N + 1_000)).otherwise, [], 'track_unmatched aus');
  ctx = await setup(rule([gt('temp', 50, { duration: 5_000 })], { track_unmatched: true }));
  assert.deepEqual((await writeEval(ctx, 'temp', 60, N + 1_000)).otherwise, [], 'Pending ist kein Unmatched (:859-863)');
});

// ---------------------------------------------------------------- Form wie serde

test('Regelform: duration/timestamp_older_than nur Ganzzahl-Millisekunden, meta nur Liste; kaputte Regel stört Geschwister nicht', async () => {
  assert.equal(ruleIsWellFormed(rule([gt('temp', 50, { duration: 5000 })])), true);
  assert.equal(ruleIsWellFormed(rule([gt('temp', 50, { duration: 'PT5M' })])), false);
  assert.equal(ruleIsWellFormed(rule([{ attribute: { name: 'temp', timestamp_older_than: 1.5 } }])), false);
  assert.equal(ruleIsWellFormed(rule([{ attribute: { name: 'temp', meta: { unit: 'x' } } }])), false);
  assert.equal(ruleIsWellFormed(rule([gt('temp', 50)], { track_unmatched: 'yes' })), false);
  const ctx = await setup(rule([gt('temp', 50, { duration: 'PT5M' })]), { id: 'bad' });
  await run(ctx, T('ruleset.save'), { id: 'good', realm: 'master', name: 'good', data: rule([gt('temp', 50)]) });
  const rep = await writeEval(ctx, 'temp', 60, N);
  assert.deepEqual(rep.rulesets, ['good']);
  assert.equal(ctx.$.doc('iot_rulesets', 'bad').match_state, undefined, 'kaputte Regel: kein Zustand geschrieben');
});
