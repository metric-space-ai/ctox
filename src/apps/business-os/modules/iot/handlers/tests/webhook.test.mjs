// Tests Webhook-Eingang über net.ingest (Kernform §16.38, Strang IOTIN) gegen die Rust-Referenz
// src/core/iot/webhook.rs (register/handle_http/ingest/value_at_path/constant_eq) und service.rs:4563-4620.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createCtx, run } from './_ctx.mjs';
import { knowledgeSim } from './_knowledge-sim.mjs';
import { constantEq, valueAtPath, webhookIdFrom } from '../_webhook.js';

const T = (op) => `ctox.iot.${op}`;
const BASE = 'https://tenant.ctox.dev/~in/iot/webhook';
const sha = (text) => createHash('sha256').update(text).digest('hex');
const ALLOWED = (s) => (s >= 200 && s <= 299) || (s >= 301 && s <= 308) || (s >= 400 && s <= 410) || s === 429;

async function setup({ knowledge, files } = {}) {
  const kn = knowledgeSim();
  const ctx = createCtx({
    caps: {
      'knowledge.query': knowledge ?? kn.query,
      'net.ingest.webhook.register': () => ({ ingest_url: BASE }),
    },
  });
  const fileCalls = [];
  const store = new Map(Object.entries(files ?? {}));
  ctx.files = {
    async get(ref) {
      fileCalls.push(ref);
      if (!store.has(ref)) throw Object.assign(new Error(`file not found: ${ref}`), { code: 'not_found' });
      return store.get(ref);
    },
  };
  await run(ctx, T('asset.upsert'), {
    id: 'a1', realm: 'master', asset_type: 'Thermostat', name: 'Serverraum',
    asset_type_info: { asset_type: 'Thermostat', attributes: [{ name: 'temp', value_descriptor: { name: 'number', base_type: 'Number' }, meta: {} }] },
  });
  const reg = await run(ctx, T('webhook.register'), { id: 'wh1', realm: 'evil', signal_ref: 'a1::temp', value_path: 'readings.temp' });
  return { ctx, kn, reg, fileCalls, points: () => kn.tables.get('iot/iot_timeseries') ?? [] };
}

let seq = 0;
const ingest = (ctx, { token, body, body_ref, sub_path = 'wh1', event_id = `ev-${++seq}`, method = 'POST', headers, received_at_ms = 1_699_999_999_500 } = {}) => {
  const payload = {
    event_id, method, path: 'webhook', sub_path, query: {}, received_at_ms,
    headers: headers ?? (token === undefined ? { 'content-type': 'application/json' } : { 'content-type': 'application/json', 'x-webhook-token': token }),
  };
  if (body !== undefined) payload.body = body;
  if (body_ref !== undefined) payload.body_ref = body_ref;
  return run(ctx, T('webhook.ingest'), payload, { actor: { id: 'net.ingest', role: 'system' } });
};
const json = (out) => JSON.parse(out.response.body);

test('register: Kernform §16.38 Nr. 1 + Ergänzung – path, command, pass_headers, default_response, aliases; kein Realm/deliver; Token nur als Digest', async () => {
  const { ctx, reg } = await setup();
  const call = ctx.$.capsTo('net.ingest.webhook.register')[0].args[0];
  assert.deepEqual(call, {
    path: 'webhook', command: 'ctox.iot.webhook.ingest', pass_headers: ['x-webhook-token'], default_response: { status: 202 },
    aliases: ['/ctox/iot/webhook/'],
  });
  assert.match(reg.token, /^[0-9a-f]{32}$/);
  assert.equal(reg.ingest_url, `${BASE}/wh1`);
  assert.equal(reg.ingest_path, `${BASE}/wh1`);
  assert.equal(reg.header, 'X-Webhook-Token');
  const row = ctx.$.doc('iot_agents', 'webhook:wh1');
  assert.equal(row.realm, 'master');
  assert.equal(row.kind, 'webhook');
  assert.equal(row.data.token_sha256, sha(reg.token));
  assert.equal(row.data.value_path, 'readings.temp');
  assert.ok(!JSON.stringify(row).includes(reg.token), 'Klartext-Token darf nicht in der Collection stehen');
  // idempotent beim Kern: zweite Registrierung ruft dieselbe Kernform erneut auf
  await run(ctx, T('webhook.register'), { id: 'wh2', signal_ref: 'a1::temp' });
  assert.deepEqual(ctx.$.capsTo('net.ingest.webhook.register')[1].args[0], call);
});

test('register: Bindungsform, ID-Zeichen, absolute ingest_url Pflicht', async () => {
  const { ctx } = await setup();
  await assert.rejects(run(ctx, T('webhook.register'), { signal_ref: 'nope' }), { code: 'invalid', message: "signal_ref must be '<asset_id>::<attribute_name>', got 'nope'" });
  await assert.rejects(run(ctx, T('webhook.register'), { signal_ref: 'a1::' }), { code: 'invalid' });
  await assert.rejects(run(ctx, T('webhook.register'), { id: 'a/b', signal_ref: 'a1::temp' }), { code: 'invalid' });
  const rel = createCtx({ caps: { 'net.ingest.webhook.register': () => ({ ingest_url: '/~in/iot/webhook' }) } });
  await assert.rejects(run(rel, T('webhook.register'), { id: 'x', signal_ref: 'a1::temp' }), { code: 'unavailable' });
  await assert.rejects(run(ctx, T('agent.configure'), { id: 'webhook:wh1', realm: 'master', name: 'x', kind: 'mqtt' }), { code: 'invalid' });
});

test('ingest: gültiges Token → 202, Datenpunkt über _timeseries mit Empfangszeit, Attribut aktualisiert', async () => {
  const { ctx, reg, points } = await setup();
  const out = await ingest(ctx, { token: reg.token, body: JSON.stringify({ readings: { temp: 27 } }) });
  assert.equal(out.response.status, 202);
  assert.deepEqual(json(out), { ok: true });
  assert.equal(out.outcome, 'Updated');
  assert.deepEqual(points(), [{ asset_id: 'a1', attribute_name: 'temp', timestamp_ms: 1_699_999_999_500, value_json: '27' }]);
  const attr = ctx.$.doc('iot_attributes', 'a1:temp');
  assert.equal(attr.data.value, 27);
  assert.equal(attr.timestamp_ms, 1_699_999_999_500);
});

test('Signaturprüfung: falsches/fehlendes Token, unbekannte ID, Token eines anderen Webhooks → 401 ohne Datenpunkt', async () => {
  const { ctx, reg, points, fileCalls } = await setup();
  const other = await run(ctx, T('webhook.register'), { id: 'wh2', signal_ref: 'a1::temp' });
  const body = JSON.stringify({ readings: { temp: 99 } });
  for (const attempt of [
    { token: 'not-the-token', body },
    { token: undefined, body },
    { token: '', body },
    { token: reg.token.slice(0, -1), body },
    { token: other.token, body }, // Token bindet genau ein Signal/einen Webhook
    { token: reg.token, body, sub_path: 'nope' },
    { token: reg.token, body, sub_path: '' },
    { token: 'x', body_ref: { ref: 'f1' } }, // unautorisiert: kein files-Lesen
  ]) {
    const out = await ingest(ctx, attempt);
    assert.equal(out.response.status, 401, JSON.stringify(attempt));
    assert.deepEqual(json(out), { ok: false });
  }
  assert.equal(points().length, 0);
  assert.deepEqual(fileCalls, []);
  // Header-Name ohne Rücksicht auf Groß/Klein (service.rs:4582), Authorization zählt nicht.
  const ok = await ingest(ctx, { headers: { 'X-Webhook-Token': reg.token }, body });
  assert.equal(ok.response.status, 202);
  const auth = await ingest(ctx, { headers: { authorization: `Bearer ${reg.token}` }, body });
  assert.equal(auth.response.status, 401);
  // Neu-Registrierung erneuert das Token: das alte gilt nicht mehr.
  const again = await run(ctx, T('webhook.register'), { id: 'wh1', signal_ref: 'a1::temp', value_path: 'readings.temp' });
  assert.equal((await ingest(ctx, { token: reg.token, body })).response.status, 401);
  assert.equal((await ingest(ctx, { token: again.token, body })).response.status, 202);
});

test('Idempotenz je event_id: Wiederzustellung schreibt keinen zweiten Datenpunkt und antwortet gleich', async () => {
  const { ctx, reg, points } = await setup();
  const body = JSON.stringify({ readings: { temp: 30 } });
  const first = await ingest(ctx, { token: reg.token, body, event_id: 'e-1' });
  const second = await ingest(ctx, { token: reg.token, body, event_id: 'e-1' });
  assert.equal(first.response.status, 202);
  assert.deepEqual(second.response, first.response);
  assert.equal(second.duplicate, true);
  assert.equal(points().length, 1);
  await ingest(ctx, { token: reg.token, body, event_id: 'e-2' });
  assert.equal(points().length, 2);
  // abgewiesenes Ereignis (Wert null) bleibt bei Wiederzustellung abgewiesen
  const nul = JSON.stringify({ readings: { temp: null } });
  assert.equal((await ingest(ctx, { token: reg.token, body: nul, event_id: 'e-3' })).response.status, 400);
  const dup = await ingest(ctx, { token: reg.token, body: nul, event_id: 'e-3' });
  assert.equal(dup.response.status, 400);
  assert.equal(dup.duplicate, true);
  const events = ctx.$.doc('iot_agents', 'webhook:wh1').data.events;
  assert.deepEqual(events.map((e) => [e.id, e.status]), [['e-1', 202], ['e-2', 202], ['e-3', 400]]);
});

test('Idempotenz: vorübergehender Fehler gibt den Anspruch frei, Wiederzustellung schreibt dann genau einmal', async () => {
  const kn = knowledgeSim();
  let down = true;
  const { ctx, reg } = await setup({ knowledge: (args) => (down ? { ok: false, error: 'engine busy' } : kn.query(args)) });
  const body = JSON.stringify({ readings: { temp: 21 } });
  const failed = await ingest(ctx, { token: reg.token, body, event_id: 'e-t' });
  assert.equal(failed.response.status, 400);
  assert.equal(ctx.$.doc('iot_agents', 'webhook:wh1').data.events.length, 0);
  down = false;
  kn.query({ argv: ['data', 'create', '--domain', 'iot', '--key', 'iot_timeseries'] });
  const retry = await ingest(ctx, { token: reg.token, body, event_id: 'e-t' });
  assert.equal(retry.response.status, 202);
  assert.equal(retry.duplicate, undefined);
  await ingest(ctx, { token: reg.token, body, event_id: 'e-t' });
  assert.equal(kn.tables.get('iot/iot_timeseries').length, 1);
});

test('Body vs. body_ref: großer Körper kommt über ctx.files.get, inline ohne files-Zugriff', async () => {
  const big = JSON.stringify({ readings: { temp: 42.5 }, pad: 'x'.repeat(70 * 1024) });
  const ref = sha(big);
  const { ctx, reg, points, fileCalls } = await setup({
    files: { [ref]: new Blob([big], { type: 'application/json' }), bytes: new TextEncoder().encode('{"readings":{"temp":7}}'), bad: new Uint8Array([0xff, 0xfe]) },
  });
  const inline = await ingest(ctx, { token: reg.token, body: '{"readings":{"temp":1}}' });
  assert.equal(inline.response.status, 202);
  assert.deepEqual(fileCalls, []);
  const viaRef = await ingest(ctx, { token: reg.token, body_ref: { ref } });
  assert.equal(viaRef.response.status, 202);
  assert.deepEqual(fileCalls, [ref]);
  const viaBytes = await ingest(ctx, { token: reg.token, body_ref: { ref: 'bytes' } });
  assert.equal(viaBytes.response.status, 202);
  assert.deepEqual(points().map((p) => p.value_json), ['1', '42.5', '7']);
  // ungültiges UTF-8 → 400 bad request (read_to_string), Form von body_ref falsch → 400
  assert.deepEqual(json(await ingest(ctx, { token: reg.token, body_ref: { ref: 'bad' } })), { ok: false, error: 'bad request' });
  assert.equal((await ingest(ctx, { token: reg.token, body_ref: 'f' })).response.status, 400);
  // fehlender files-Datensatz: 400, Anspruch freigegeben (Wiederzustellung darf neu lesen)
  const missing = await ingest(ctx, { token: reg.token, body_ref: { ref: 'gone' }, event_id: 'e-gone' });
  assert.equal(missing.response.status, 400);
  assert.ok(!ctx.$.doc('iot_agents', 'webhook:wh1').data.events.some((e) => e.id === 'e-gone'));
});

test('HTTP-Abbildung wie service.rs: Methode, JSON, Pfad; Antworten im §16.38-Rahmen', async () => {
  const { ctx, reg, points } = await setup();
  const outs = [
    [await ingest(ctx, { token: reg.token, method: 'GET', body: '{}' }), 404, { error: 'not found' }],
    [await ingest(ctx, { token: reg.token, body: '{bad' }), 400, { ok: false, error: 'invalid json' }],
    [await ingest(ctx, { token: reg.token }), 400, { ok: false, error: 'invalid json' }], // leerer Körper
    [await ingest(ctx, { token: reg.token, body: '{"readings":{}}' }), 400, { ok: false }], // kein Wert am Pfad
    [await ingest(ctx, { token: reg.token, body: '{"readings":{"temp":"heiß"}}' }), 400, { ok: false }], // Koerzierung
    [await ingest(ctx, { token: reg.token, body: '{"readings":{"temp":3}}', sub_path: 'wh1/' }), 202, { ok: true }],
  ];
  for (const [out, status, body] of outs) {
    assert.equal(out.response.status, status);
    assert.deepEqual(json(out), body);
    assert.ok(ALLOWED(out.response.status));
    assert.deepEqual(Object.keys(out.response.headers), ['content-type']);
    assert.ok(new TextEncoder().encode(out.response.body).length <= 64 * 1024);
  }
  assert.equal(points().length, 1);
});

test('Hilfen: value_at_path, constant_eq, Webhook-ID aus sub_path (webhook.rs-Tests)', () => {
  const p = { data: { temp: 23.5 }, flat: 7, list: [1, 2] };
  assert.equal(valueAtPath(p, 'data.temp'), 23.5);
  assert.equal(valueAtPath(p, 'flat'), 7);
  assert.equal(valueAtPath(p, null), p);
  assert.equal(valueAtPath(p, ''), p);
  assert.equal(valueAtPath(p, 'data.missing'), undefined);
  assert.equal(valueAtPath(p, 'list.0'), undefined); // serde Value::get(&str) indiziert keine Arrays
  assert.ok(constantEq('abc', 'abc'));
  assert.ok(!constantEq('abc', 'abd'));
  assert.ok(!constantEq('abc', 'ab'));
  assert.equal(webhookIdFrom('wh1'), 'wh1');
  assert.equal(webhookIdFrom('/wh1/?x=1'), 'wh1');
  assert.equal(webhookIdFrom('a/b'), 'b');
});

test('register: zugestellter Befehlstyp hat einen Handler der App; Altpfad /ctox/iot/webhook/<id> (Alias) wird angenommen', async () => {
  const { ctx, reg, points } = await setup();
  const { command } = ctx.$.capsTo('net.ingest.webhook.register')[0].args[0];
  const { existsSync } = await import('node:fs');
  assert.ok(existsSync(new URL(`../${command}.js`, import.meta.url)), `Handler ${command}.js fehlt`);
  // Zustellung über den Alias: gleicher Befehl, Teilpfad = Webhook-ID
  const payload = {
    event_id: 'ev-alias', method: 'POST', path: '/ctox/iot/webhook/', sub_path: 'wh1', query: {}, received_at_ms: 1_699_999_999_500,
    headers: { 'content-type': 'application/json', 'x-webhook-token': reg.token }, body: JSON.stringify({ readings: { temp: 23.5 } }),
  };
  const out = await run(ctx, command, payload, { actor: { id: 'net.ingest', role: 'system' } });
  assert.equal(out.response.status, 202);
  assert.equal(points().length, 1);
});
