import {testLocks} from './web-locks.mjs';
// node --test test/  (Node ≥ 22; ohne IndexedDB läuft der In-Memory-Speicher mit gleicher Schnittstelle)
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import {
  createStubKernel as createContractKernel, createShellKernel, compileQuery, runCompiledQuery, CtoxError, sha256Js, uuidv7,
} from '../kernel.mjs';
import { createAppHost } from '../testkit/app-host.mjs';
const createStubKernel=options=>createContractKernel({locks:testLocks,...options});

let n = 0;
async function mk(o = {}) {
  const k = createStubKernel({locks:testLocks, instanceId: `t${process.pid}-${++n}`, userId: 'u1', latencyMs: 5, ...o });
  await k.ready;
  return k;
}
const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms));
async function until(pred, ms = 3000) {
  const t0 = Date.now();
  while (!(await pred())) { if (Date.now() - t0 > ms) throw new Error('until: Zeitüberschreitung'); await tick(5); }
}
const moduleOf = (impl) => ({ mount: impl });

test('Hilfen: sha256 (JS-Rückfall) und UUIDv7 monoton', () => {
  for (const s of ['', 'abc', 'x'.repeat(1000)]) {
    assert.equal(sha256Js(new TextEncoder().encode(s)), createHash('sha256').update(s).digest('hex'));
  }
  const ids = Array.from({ length: 2000 }, uuidv7);
  assert.deepEqual([...ids].sort(), ids);
  assert.match(ids[0], /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
});

test('Mango-Teilmenge: Operatoren, null ≠ fehlend, Sortierung mit id-Gleichstand', () => {
  const docs = [
    { id: 'a', n: 3, s: 'Alpha', tags: ['x', 'y'], v: null, items: [{ k: 1 }, { k: 5 }] },
    { id: 'b', n: 1, s: 'beta', tags: ['y'] },
    { id: 'c', n: 2, s: 'Gamma', v: 0, items: [{ k: 2 }] },
    { id: 'd', n: 2, s: 'delta', v: 'z' },
  ];
  const q = (query) => runCompiledQuery(docs, compileQuery(query)).map((d) => d.id);
  assert.deepEqual(q({ selector: { v: null } }), ['a']);
  assert.deepEqual(q({ selector: { v: { $eq: null } } }), ['a']);
  assert.deepEqual(q({ selector: { v: { $exists: false } } }), ['b']);
  assert.deepEqual(q({ selector: { v: { $exists: true } } }), ['a', 'c', 'd']);
  assert.deepEqual(q({ selector: { v: { $ne: null } } }), ['b', 'c', 'd']);
  assert.deepEqual(q({ selector: { n: { $gt: 1, $lte: 2 } } }), ['c', 'd']);
  assert.deepEqual(q({ selector: { n: { $in: [1, 3] } } }), ['a', 'b']);
  assert.deepEqual(q({ selector: { n: { $nin: [1, 3] } } }), ['c', 'd']);
  assert.deepEqual(q({ selector: { s: { $regex: '^[ab]', $options: 'i' } } }), ['a', 'b']);
  assert.deepEqual(q({ selector: { s: /^g/i } }), ['c']);
  assert.deepEqual(q({ selector: { tags: { $elemMatch: { $eq: 'x' } } } }), ['a']);
  assert.deepEqual(q({ selector: { items: { $elemMatch: { k: { $gte: 2 } } } } }), ['a', 'c']);
  assert.deepEqual(q({ selector: { $or: [{ n: 1 }, { s: 'delta' }] } }), ['b', 'd']);
  assert.deepEqual(q({ selector: { $and: [{ n: 2 }, { $not: { s: 'delta' } }] } }), ['c']);
  assert.deepEqual(q({ selector: { n: { $not: { $gt: 1 } } } }), ['b']);
  assert.deepEqual(q({ selector: {}, sort: [{ n: 'desc' }] }), ['a', 'c', 'd', 'b']); // c/d gleich → nach id
  assert.deepEqual(q({ sort: [{ n: 'asc' }, { s: 'desc' }] }), ['b', 'd', 'c', 'a']);
  assert.deepEqual(q({ sort: [{ v: 'asc' }] }), ['b', 'a', 'c', 'd']); // fehlend < null < Zahl < String
  assert.deepEqual(q({ sort: [{ n: 'asc' }], skip: 1, limit: 2 }), ['c', 'd']);
  for (const bad of [{ selector: { n: { $foo: 1 } } }, { where: {} }, { selector: { s: { $regex: 'x', $options: 'g' } } },
    { selector: { a: { $elemMatch: { b: { $elemMatch: { c: 1 } } } } } }, { sort: [['n', 'asc']] }]) {
    assert.throws(() => compileQuery(bad), (e) => e instanceof CtoxError && e.code === 'invalid');
  }
});

test('query/get live, lokal, ohne Zeilendeckel; Änderungen erreichen alle Abonnenten', async () => {
  const k = await mk();
  const a = k.appApi('writer', '1');
  const b = k.appApi('reader', '1');
  const lq = b.query('notes', { selector: { done: false }, sort: [{ prio: 'desc' }] });
  const seen = [];
  const off = lq.subscribe((rows) => seen.push(rows.length));
  const single = b.get('notes', 'n7');
  let singleVal;
  single.subscribe((d) => { singleVal = d; });
  for (let i = 0; i < 600; i++) a.put('notes', { id: `n${i}`, prio: i % 7, done: i % 3 === 0 });
  assert.equal(lq.get().length, 400, 'lokal sofort sichtbar, kein 200er-Deckel');
  await until(() => seen.at(-1) === 400 && singleVal?.id === "n7");
  assert.equal(seen.at(-1), 400);
  assert.equal(singleVal.id, 'n7');
  assert.ok(lq.get()[0].prio === 6 && lq.get()[0].id < lq.get()[1].id);
  assert.throws(() => { lq.get()[0].prio = 1; }, TypeError, 'Ergebnisse sind eingefroren');
  a.patch('notes', 'n7', { done: true });
  a.remove('notes', 'n8');
  await until(() => seen.at(-1) === 398 && singleVal?.done === true);
  assert.equal(seen.at(-1), 398);
  assert.equal(singleVal.done, true);
  off();
  await k.sim.settle();
  assert.equal((await k.sim.outbox()).length, 0);
  assert.equal(b.query('notes', {}).get().length, 599);
  await k.close();
});

test('put → accepted mit rev/seq; local committet zuerst, pending-Flag', async () => {
  const k = await mk({ latencyMs: 30 });
  const api = k.appApi('writer', '1');
  const h = api.put('notes', { id: 'x', title: 'eins' });
  assert.equal(typeof h.cancel, 'function');
  await h.local;
  const rec = await k.sim.inspect('notes', 'x');
  assert.equal(rec.pending, true);
  assert.match(rec.rev, /^op:/);
  assert.equal((await k.sim.outbox())[0].op_id, h.op_id);
  const r = await h.result;
  assert.equal(r.status, 'accepted');
  assert.match(r.rev, /^\d+-[0-9a-f]+$/);
  assert.ok(r.seq > 0);
  const after = await k.sim.inspect('notes', 'x');
  assert.equal(after.pending, false);
  assert.equal(after.rev, r.rev);
  assert.equal(k.sim.checkpoint, r.seq);
  await k.close();
});

test('rejected → Serverstand zurück, Eingabe in archive_local, Zustandszone', async () => {
  const k = await mk();
  const api = k.appApi('writer', '1');
  await k.sim.push('notes', [{ id: 's1', title: 'Server', n: 1 }]);
  assert.equal(api.get('notes', 's1').get().title, 'Server');
  await k.sim.rejectNext('revoked');
  const h = api.patch('notes', 's1', { title: 'Lokal' });
  assert.equal(api.get('notes', 's1').get().title, 'Lokal', 'optimistisch');
  const r = await h.result;
  assert.equal(r.status, 'rejected');
  assert.equal(r.reason, 'revoked');
  assert.equal(api.get('notes', 's1').get().title, 'Server');
  const arch = await k.sim.archiveLocal();
  assert.equal(arch.length, 1);
  assert.deepEqual(arch[0].body, { title: 'Lokal' });
  assert.equal(arch[0].ref, r.archived_ref);
  const z = k.status.forApp('writer').get();
  assert.equal(z.length, 1);
  assert.equal(z[0].level, 'error');
  assert.match(z[0].text, /Änderung abgelehnt: revoked/);
  // patch auf unbekanntes Dokument → not_found; neuer Datensatz ohne Serverstand verschwindet wieder
  const r2 = await api.patch('notes', 'nope', { a: 1 }).result;
  assert.equal(r2.reason, 'not_found');
  assert.equal(api.get('notes', 'nope').get(), null);
  await k.close();
});

test('migrated → archived_ref, entfernte Felder im Core-Archiv', async () => {
  const k = await mk();
  const api = k.appApi('writer', '1');
  await k.sim.migrateNext({ dropFields: ['legacy'] });
  const r = await api.put('notes', { id: 'm', title: 't', legacy: 42 }).result;
  assert.equal(r.status, 'migrated');
  assert.match(r.archived_ref, /^core_archive:/);
  assert.deepEqual({ ...api.get('notes', 'm').get() }, { id: 'm', title: 't' });
  await k.close();
});

test('delete mit veralteter Basis → conflict (§16.6); Folge-Write nennt prev_op_id', async () => {
  const k = await mk();
  const api = k.appApi('writer', '1');
  await api.put('notes', { id: 'd', v: 1 }).result;
  await k.sim.setLink({ state: 'offline' });
  const p1 = api.patch('notes', 'd', { v: 2 });
  const p2 = api.patch('notes', 'd', { v: 3 });
  await p2.local;
  const ob = await k.sim.outbox();
  assert.equal(ob[1].prev_op_id, p1.op_id);
  assert.equal(ob[1].base_rev, `op:${p1.op_id}`);
  assert.equal(k.appApi('writer', '1').metrics.get().outbox.count, 2);
  await k.sim.setLink({ state: 'online' });
  assert.equal((await p2.result).status, 'accepted');
  await k.sim.setLink({ state: 'offline' });
  const del = api.remove('notes', 'd');
  await del.local;
  await k.sim.push('notes', [{ id: 'd', v: 99 }]); // Core ändert parallel, Zustellung erst online
  assert.equal(api.metrics.get().backlog, 1);
  await k.sim.setLink({ state: 'online' });
  const r = await del.result;
  assert.equal(r.status, 'rejected');
  assert.equal(r.reason, 'conflict');
  assert.equal(api.get('notes', 'd').get().v, 99);
  await k.close();
});

test('lokale Schemaprüfung: invalid → local lehnt ab, nichts geschrieben; result lehnt nie ab', async () => {
  const k = await mk();
  await k.sim.publishAppVersion('todo', '1', {
    collections: { tasks: { schema: { type: 'object', required: ['title'], properties: { title: { type: 'string' }, n: { type: 'integer' } } } } },
  });
  const api = k.appApi('todo', '1');
  const bad = api.put('tasks', { id: 't1', title: 5 });
  await assert.rejects(bad.local, (e) => e instanceof CtoxError && e.code === 'invalid' && e.path === 'title');
  assert.deepEqual(await bad.result, { status: 'rejected', reason: 'invalid' });
  const noId = api.put('tasks', { title: 'x' });
  await assert.rejects(noId.local, (e) => e.code === 'invalid' && e.path === 'id');
  await assert.rejects(api.put('tasks', { id: 't2' }).local, (e) => e.code === 'invalid' && e.path === 'title');
  await assert.rejects(api.put('tasks', { id: 't3', title: 'x', when: new Date() }).local, (e) => e.path === 'when');
  assert.equal((await k.sim.outbox()).length, 0);
  assert.equal(api.query('tasks', {}).get().length, 0);
  assert.equal((await api.put('tasks', { id: 't4', title: 'ok', n: 2 }).result).status, 'accepted');
  await k.close();
});

test('Outbox überlebt Neustart (gleiche Instanz+Nutzer), cancel nur vor dem Senden', async () => {
  const opts = { instanceId: `persist-${process.pid}`, userId: 'anna', latencyMs: 5 };
  const k1 = createStubKernel(opts);
  await k1.ready;
  assert.equal(k1.dbName, `ctox-persist-${process.pid}-anna`);
  await k1.sim.setLink({ state: 'offline' });
  const api = k1.appApi('writer', '1');
  const a = api.put('notes', { id: 'a', t: 1 });
  const b = api.put('notes', { id: 'b', t: 2 });
  await Promise.all([a.local, b.local]);
  assert.equal(b.cancel(), true);
  assert.deepEqual(await b.result, { status: 'rejected', reason: 'cancelled' });
  assert.equal(api.get('notes', 'b').get(), null);
  let resolved = false;
  a.result.then(() => { resolved = true; });
  await tick(30);
  assert.equal(resolved, false, 'result bleibt offen, solange offline');
  await k1.close();

  const k2 = createStubKernel({locks:testLocks, ...opts });
  await k2.ready;
  const rec = await k2.sim.inspect('notes', 'a');
  assert.ok(rec, 'Dokument lokal vorhanden');
  await k2.sim.settle();
  assert.equal((await k2.sim.inspect('notes', 'a')).pending, false);
  assert.equal((await k2.sim.outbox()).length, 0);
  await k2.close();
});

test('command: Dokument mit gepushtem Status queued → accepted → running → done | failed', async () => {
  const k = await mk();
  const api = k.appApi('writer', '1');
  k.sim.onCommand('notes.export', async (payload) => ({ count: payload.n * 2 }));
  const c = api.command('notes.export', { n: 21 });
  const states = [];
  c.status.subscribe((s) => states.push(s.status));
  assert.equal(c.status.get().status, 'queued', 'kein Warten auf Annahme');
  await until(() => c.status.get().status === 'done');
  assert.deepEqual(states, ['queued', 'accepted', 'running', 'done']);
  assert.deepEqual(c.status.get().result, { count: 42 });
  const doc = api.get('business_commands', c.id).get();
  assert.equal(doc.type, 'notes.export');
  await k.sim.failCommand('notes.export', 'kaputt');
  const c2 = api.command('notes.export', { n: 1 });
  await until(() => c2.status.get().status === 'failed');
  assert.equal(c2.status.get().error, 'kaputt');
  await k.sim.rejectNext('forbidden');
  const c3 = api.command('notes.export', {});
  await until(() => c3.status.get().status === 'failed');
  assert.equal(c3.status.get().error, 'forbidden');
  await k.sim.settle();
  await k.close();
});

test('App-Aktivierung received → fetched → data-ready → activated; Remount (testkit) mit snapshot/restored, 9-s-Rückfall, needs_update', async () => {
  const k = await mk();
  const host = createAppHost(k, { mountTimeoutMs: 200 });
  const events = [];
  k.activation.onChange((e) => events.push(`${e.ver}:${e.state}`));
  host.registerModule('notes', '1', moduleOf((root, api, ctx) => {
    root.log.push(`v1 mount restored=${JSON.stringify(ctx.restored)}`);
    return { unmount() { root.log.push('v1 unmount'); }, snapshot: () => ({ draft: 'halb getippt', ver: api.app.ver }) };
  }));
  await k.sim.publishAppVersion('notes', '1', {});
  assert.deepEqual(events, ['1:received', '1:fetched', '1:data-ready', '1:activated']);
  const root = { log: [] };
  const m = await host.mount(root, 'notes');
  assert.equal(m.ver, '1');

  // v2: Code aus Artefakten (data:-Import), Wasserstand liegt in der Zukunft → wartet auf data-ready
  const head = await k.sim.coreHead();
  const v2src = `export function mount(root, api, ctx) {
    root.log.push('v2 mount'); root.restored = ctx.restored; root.api = api;
    return { unmount() { root.log.push('v2 unmount'); }, snapshot() { return { from: 'v2' }; } };
  }`;
  const p = k.sim.publishAppVersion('notes', '2', { files: { 'index.js': v2src, 'style.css': 'p{}' }, migratedAtSeq: head + 4 });
  await until(() => k.activation.get('notes').get()?.state === 'fetched' && k.activation.get('notes').get()?.target_ver === '2');
  await tick(20);
  assert.equal(k.activation.get('notes').get().state, 'fetched', 'wartet auf Checkpoint ≥ migrated_at_seq');
  await k.sim.push('other', [{ id: 'o1' }, { id: 'o2' }]);
  const st = await p;
  assert.equal(st.state, 'activated');
  assert.equal(st.activated_ver, '2');
  await host.idle();
  assert.deepEqual(root.restored, { draft: 'halb getippt', ver: '1' });
  assert.equal(m.ver, '2');
  assert.deepEqual(root.log, ['v1 mount restored=undefined', 'v1 unmount', 'v2 mount']);
  const ws = await k.sim.windowStateRows();
  assert.deepEqual(ws.find((r) => r.value?.next_ver === '2').value.snapshot, { draft: 'halb getippt', ver: '1' });
  assert.ok(k.sim.activationReports.some((r) => r.app_ver === '2' && r.state === 'activated'));
  const man = await k.artifacts.manifest('notes', '2');
  assert.equal(await (await k.artifacts.read(man.files['index.js'])).text(), v2src);

  // v3: mount wirft → testkit mountet v2 wieder (mit Snapshot), reportFailed setzt den Zeiger zurück
  host.registerModule('notes', '3', moduleOf(() => { throw new Error('boom'); }));
  await k.sim.publishAppVersion('notes', '3', {});
  await until(() => k.activation.get('notes').get()?.state === 'failed');
  await host.idle();
  assert.equal(m.ver, '2');
  assert.equal(k.activation.get('notes').get().activated_ver, '2');
  assert.match(k.activation.get('notes').get().error, /reported: mount_failed: boom/);
  assert.ok(k.status.forApp('notes').get().some((e) => e.code === 'app.remount_failed' && e.source === 'shell'));
  assert.deepEqual(root.restored, { from: 'v2' });
  assert.ok(k.sim.activationReports.some((r) => r.app_ver === '3' && r.state === 'failed'));

  // v4: mountet nicht binnen mountTimeoutMs → Rückfall
  host.registerModule('notes', '4', moduleOf(() => new Promise(() => {})));
  const t0 = Date.now();
  await k.sim.publishAppVersion('notes', '4', {});
  await until(() => k.activation.get('notes').get()?.target_ver === '4' && k.activation.get('notes').get()?.state === 'failed');
  assert.ok(Date.now() - t0 < 1000, 'Rückfall binnen Frist');
  await host.idle();
  assert.equal(m.ver, '2');

  // fehlende Fähigkeit → failed{needs_update}, Zustandsstrom
  const r5 = await k.sim.publishAppVersion('notes', '5', { capabilitiesRequired: ['cap.gpu'] });
  assert.equal(r5.state, 'failed');
  assert.match(r5.error, /^needs_update/);
  assert.ok(k.status.forApp('notes').get().some((e) => e.code === 'app.needs_update'));
  await assert.rejects(k.sim.publishAppVersion('notes', '6', { module: {} }), (e) => e.code === 'invalid');
  await host.dispose();
  await k.close();
});

test('activation.reportFailed: Zeiger zurück auf letzte funktionierende Version, app_activation failed', async () => {
  const k = await mk();
  await k.sim.publishAppVersion('a', '1', {});
  await k.sim.publishAppVersion('a', '2', {});
  assert.equal(k.activation.get('a').get().activated_ver, '2');
  const r = await k.activation.reportFailed('a', '2', 'render crash');
  assert.equal(r.state, 'failed');
  assert.equal(r.activated_ver, '1');
  assert.equal(r.target_ver, '2');
  assert.equal(k.apps.get()[0].activated_ver, '1');
  assert.deepEqual(k.sim.activationReports.at(-1), { app_id: 'a', app_ver: '2', state: 'failed', at_seq: k.sim.checkpoint, error: 'reported: render crash' });
  await tick(20);
  assert.equal(k.activation.get('a').get().activated_ver, '1', 'keine erneute Aktivierung der gemeldeten Version');
  await k.close();
});

test('Artefakt mit falschem Hash: einmal neu holen, erst wiederholter Fehlschlag → failed (§16.15a)', async () => {
  const k = await mk();
  await k.sim.corruptNextFetch(1);
  const ok = await k.sim.publishAppVersion('x', '1', { files: { 'index.js': 'export const a = 1;' } });
  assert.equal(ok.state, 'activated');
  assert.equal(k.sim.refetches, 1);
  await k.sim.corruptNextFetch(2);
  const bad = await k.sim.publishAppVersion('x', '2', { files: { 'index.js': 'export const a = 2;' } });
  assert.equal(bad.state, 'failed');
  assert.match(bad.error, /^hash_mismatch/);
  await k.close();
});

test('kernel.apps liefert layout und styles aus dem Manifest (§16.19 Nr. 6)', async () => {
  const k = await mk();
  await k.sim.publishAppVersion('crm', '1', {
    title: 'CRM', styles: ['styles/app.css'],
    layout: { icon_svg: '<svg/>', category: 'Vertrieb', default_width: 900, default_height: 640, min_width: 480, presentation: 'window' },
  });
  const [app] = k.apps.get();
  assert.deepEqual(app.layout, {
    icon_asset: null, icon_svg: '<svg/>', category: 'Vertrieb', frame_palette: null, default_width: 900,
    default_height: 640, min_width: 480, min_height: null, presentation: 'window',
  });
  assert.deepEqual(app.styles, ['styles/app.css']);
  await k.sim.publishAppVersion('crm', '2', {}); // erbt layout/styles der Vorversion
  await until(() => k.apps.get()[0].activated_ver === '2');
  assert.equal(k.apps.get()[0].layout.category, 'Vertrieb');
  await k.close();
});

test('windowState je Tab und Nutzer; shell_preferences nur eigene Zeile (§16.19 Nr. 3)', async () => {
  const opts = { instanceId: `ws-${process.pid}`, userId: 'mia', latencyMs: 5, multiTab: false };
  const t1 = createStubKernel({locks:testLocks, ...opts, tabKey: 'tab-A' });
  await t1.ready;
  await t1.windowState.put('layout', { open: ['crm'], z: [1] });
  assert.deepEqual(await t1.windowState.get('layout'), { open: ['crm'], z: [1] });
  await assert.rejects(t1.windowState.put('bad', () => 1), (e) => e.code === 'invalid');
  await t1.close();
  const t1b = createStubKernel({locks:testLocks, ...opts, tabKey: 'tab-A' }); // Neuladen desselben Tabs
  await t1b.ready;
  assert.deepEqual(await t1b.windowState.get('layout'), { open: ['crm'], z: [1] });
  await t1b.close();
  const t2 = createStubKernel({locks:testLocks, ...opts, tabKey: 'tab-B' });
  await t2.ready;
  assert.equal(await t2.windowState.get('layout'), null, 'anderer Tab sieht nichts');
  const api = t2.appApi('shell', '1');
  assert.equal((await api.put('shell_preferences', { id: 'mia', pins: ['crm'], launcher: ['crm', 'mail'] }).result).status, 'accepted');
  await assert.rejects(api.put('shell_preferences', { id: 'tom', pins: [] }).local, (e) => e.code === 'forbidden');
  await t2.sim.push('shell_preferences', [{ id: 'tom', pins: ['x'] }]);
  assert.deepEqual(api.query('shell_preferences', {}).get().map((d) => d.id), ['mia']);
  assert.equal(api.get('shell_preferences', 'tom').get(), null);
  await t2.close();
});

test('registerHeader nimmt ein Element; link.state nur online|offline (§16.19 Nr. 5, 7)', async () => {
  const k = await mk();
  const api = k.appApi('notes', '1');
  const el = { tagName: 'DIV', id: 'kopf' }; // in Node gibt es kein HTMLElement; im Browser wird instanceof geprüft
  api.shell.registerHeader(el);
  assert.equal(k.shell.window('notes').state.get().header, el, 'Referenz, keine Kopie');
  assert.throws(() => api.shell.registerHeader('<h1>'), (e) => e.code === 'invalid');
  await assert.rejects(k.sim.setLink({ state: 'connecting' }), (e) => e.code === 'invalid');
  assert.equal(api.metrics.get().link.state, 'online');
  const c = api.command('crm.touch', { a: 1 }, { record_id: 'lead-7' });
  await c.local;
  assert.equal(api.get('business_commands', c.id).get().record_id, 'lead-7');
  await k.sim.settle();
  await k.close();
});

test('Generationen (§16.4/§16.16): name@g2, alter Code liest g1, Doppelschreiben, g1 nach Frist verworfen', async () => {
  const k = await mk({ generationGraceMs: 150 });
  await k.sim.publishAppVersion('crm', '1', {});
  const v1 = k.appApi('crm', '1');
  await v1.put('leads', { id: 'l1', name: 'Ada Lovelace' }).result;
  await v1.put('leads', { id: 'l2', name: 'Alan Turing' }).result;
  const st = await k.sim.publishAppVersion('crm', '2', {
    incompatible: ['leads'],
    migrate: { leads: (d) => { const [first, last] = d.name.split(' '); return { id: d.id, first, last }; } },
  });
  assert.equal(st.state, 'activated');
  assert.ok(k.sim.activationReports.some((r) => r.app_id === 'crm' && r.app_ver === '2' && r.state === 'data-ready'));
  const v2 = k.appApi('crm', '2');
  assert.deepEqual(v2.query('leads', { sort: [{ last: 'asc' }] }).get().map((d) => d.last), ['Lovelace', 'Turing']);
  assert.equal(v1.query('leads', {}).get()[0].name, 'Ada Lovelace', 'alter Code liest g1');
  assert.ok((await k.sim.inspect('leads@g2', 'l1')).data.first === 'Ada');
  await v1.put('leads', { id: 'l3', name: 'Grace Hopper' }).result;
  await until(() => v2.get('leads', 'l3').get()?.last === 'Hopper');
  v2.setVisible(['leads']);
  assert.deepEqual(k.sim.lastVisible.collections, ['leads@g2']);
  await k.sim.settle();
  // §16.16: g1 bleibt, bis v2 die Frist ohne Rückfall lief; dann verwirft der Client sie
  assert.equal(v1.query('leads', {}).get().length, 3);
  await until(() => v1.query('leads', {}).get().length === 0, 2000);
  assert.equal(await k.sim.inspect('leads', 'l1'), undefined);
  assert.equal(v2.query('leads', {}).get().length, 3);
  await k.close();
});

test('Zustandszone: report/clear je App, live, gleicher code ersetzt', async () => {
  const k = await mk();
  const a = k.appApi('a', '1');
  const b = k.appApi('b', '1');
  const lists = [];
  k.status.forApp('a').subscribe((l) => lists.push(l.map((e) => e.text)));
  a.status.report({ level: 'warn', code: 'sync', text: 'eins' });
  a.status.report({ level: 'warn', code: 'sync', text: 'zwei' });
  b.status.report({ level: 'info', code: 'x', text: 'b' });
  await tick();
  assert.deepEqual(lists.at(-1), ['zwei']);
  assert.equal(k.status.get().length, 2);
  assert.throws(() => a.status.report({ level: 'loud', code: 'c', text: '' }), (e) => e.code === 'invalid');
  a.status.clear('sync');
  await tick();
  assert.deepEqual(lists.at(-1), []);
  assert.equal(k.status.get().length, 1);
  await k.close();
});

test('Dateien: put (Blob + Outbox + files-Dokument), get aus Cache, nach Räumung über „Spur 4“', async () => {
  const k = await mk();
  const api = k.appApi('docs', '1');
  const blob = new Blob(['Hallo Datei'], { type: 'text/plain' });
  const h = api.files.put(blob, { name: 'gruss.txt' });
  const progress = [];
  h.upload.progress.subscribe((p) => progress.push(p.sent));
  const ref = await h.ref;
  assert.equal(ref, createHash('sha256').update('Hallo Datei').digest('hex'));
  const r = await h.result;
  assert.equal(r.status, 'accepted');
  assert.equal(r.ref, ref);
  assert.deepEqual(h.upload.progress.get(), { sent: 11, total: 11 });
  assert.equal(progress.at(-1), 11);
  const meta = api.query('files', { selector: { ref } }).get()[0];
  assert.equal(meta.name, 'gruss.txt');
  assert.equal(meta.owner_app, 'docs');
  assert.equal(await (await api.files.get(ref)).text(), 'Hallo Datei');
  assert.equal(await k.sim.evictBlobs(), 1);
  assert.equal(await (await api.files.get(ref)).text(), 'Hallo Datei');
  await assert.rejects(api.files.get('0'.repeat(64)), (e) => e.code === 'not_found');
  await k.close();
});

test('api.shell, api.actor, setVisible: navigate unbekannt → Zustandszone', async () => {
  const k = await mk();
  await k.sim.publishAppVersion('mail', '1', {});
  const a = k.appApi('notes', '1');
  const mail = k.appApi('mail', '1');
  const routes = [];
  mail.shell.onRoute((r) => routes.push(r));
  assert.equal(a.shell.navigate('mail', { thread: 't1' }), true);
  assert.deepEqual(routes[0].params, { thread: 't1' });
  assert.equal(a.shell.navigate('gibtsnicht', {}), false);
  const z = k.status.forApp('notes').get();
  assert.equal(z[0].code, 'shell.navigate.unknown_target');
  assert.equal(z[0].level, 'error');
  k.sim.setTranslations('de', { hello: 'Hallo' });
  assert.equal(a.shell.t('hello'), 'Hallo');
  assert.equal(a.shell.locale.get(), 'de');
  a.shell.setTitle('Notizen');
  assert.equal(createShellKernel(k).window('notes').state.get().title, 'Notizen');
  const perms = [];
  a.actor.permissions.subscribe((p) => perms.push(p));
  assert.equal(a.actor.role, 'admin');
  k.sim.setPermissions(['apps.source.view']);
  await tick();
  assert.deepEqual(perms.at(-1), ['apps.source.view']);
  a.setVisible(['notes', 'files']);
  assert.deepEqual(k.sim.lastVisible.collections, ['notes', 'files']);
  assert.equal(a.version, '4.0');
  assert.deepEqual({ ...a.app }, { id: 'notes', ver: '1' });
  await k.close();
});

test('Shell-Kernel: apps mit Aktivierungszustand, source.* rechtegeprüft, draft.publish', async () => {
  const k = await mk();
  const sk = createShellKernel(k);
  assert.equal(sk.apps, k.apps);
  const src = 'export function mount(){ return { unmount(){} }; }';
  await k.sim.publishAppVersion('leads', '1', { files: { 'index.js': src, 'README.md': '# Leads' }, title: 'Leads' });
  const apps = sk.apps.get();
  assert.deepEqual(apps.map((x) => [x.id, x.title, x.state, x.activated_ver]), [['leads', 'Leads', 'activated', '1']]);
  assert.deepEqual(sk.leader.get().isLeader, true);
  assert.ok(sk.leader.get().leader_gen >= 1);
  const s = sk.source('leads');
  assert.deepEqual(await s.list(), ['README.md', 'index.js']);
  assert.equal(await s.read('README.md'), '# Leads');
  await s.draft.write('README.md', '# Leads v2');
  await s.draft.setCollections({ leads: { add: ['score'] } });
  assert.equal(await s.read('README.md'), '# Leads v2', 'Entwurf überlagert');
  const stale = await s.draft.publish({ base_app_ver: '0' }).result;
  assert.deepEqual(stale, { status: 'rejected', reason: 'stale_base' });
  const pub = await s.draft.publish({ base_app_ver: '1' }).result;
  assert.equal(pub.status, 'accepted');
  await until(() => sk.apps.get()[0].activated_ver === pub.rev);
  assert.equal(await s.read('README.md', pub.rev), '# Leads v2');
  k.sim.setPermissions([]);
  await assert.rejects(s.list(), (e) => e.code === 'forbidden');
  await assert.rejects(s.draft.write('a.js', ''), (e) => e.code === 'forbidden');
  assert.deepEqual(await s.draft.publish({ base_app_ver: pub.rev }).result, { status: 'rejected', reason: 'forbidden' });
  await k.close();
});

test('Shell-Tausch: stopShell → snapshot → startShell(kernel, {restored}); defekte Shell → Rückfall', async () => {
  const k = await mk({ shellStartTimeoutMs: 100 });
  const log = [];
  const shellA = {
    startShell(kernel, { restored }) { log.push(['A start', restored]); assert.ok(createShellKernel(kernel).apps); },
    stopShell() { log.push(['A stop']); return { openWindows: ['notes'], focus: 'notes' }; },
  };
  const shellB = {
    startShell(kernel, { restored }) { log.push(['B start', restored]); },
    stopShell() { log.push(['B stop']); return { openWindows: ['mail'] }; },
  };
  await assert.rejects(k.startShell({startShell:()=>new Promise(()=>{}),stopShell(){}}),e=>e.code==='unavailable'&&/100 ms/.test(e.message));
  const broken = { startShell() { throw new Error('Shell kaputt'); }, stopShell() {} };
  await k.sim.startShell(shellA);
  const r = await k.sim.swapShell(shellB);
  assert.equal(r.swapped, true);
  assert.deepEqual(log, [['A start', undefined], ['A stop'], ['B start', { openWindows: ['notes'], focus: 'notes' }]]);
  const r2 = await k.sim.swapShell(broken);
  assert.equal(r2.swapped, false);
  assert.deepEqual(log.slice(3), [['B stop'], ['B start', { openWindows: ['mail'] }]]);
  assert.ok(k.status.forApp('shell').get().some((e) => e.code === 'shell.swap_failed'));
  assert.deepEqual(await k.windowState.get('shell'), { openWindows: ['mail'] });
  await k.close();
});

test('Mehr-Tab: Leser schreibt in gemeinsame Outbox, Leiter sendet; Leiterwechsel; local_seq-Abgleich', async () => {
  const opts = { instanceId: `tabs-${process.pid}`, userId: 'u', latencyMs: 5 };
  const t1 = createStubKernel(opts);
  await t1.ready;
  const t2 = createStubKernel(opts);
  await t2.ready;
  assert.equal(t1.isLeader, true);
  assert.equal(t2.isLeader, false);
  const a1 = t1.appApi('notes', '1');
  const a2 = t2.appApi('notes', '1');
  const r = await a2.put('notes', { id: 'from-reader', v: 1 }).result;
  assert.equal(r.status, 'accepted');
  await until(() => a1.get('notes', 'from-reader').get()?.v === 1);
  await until(async () => (await t2.sim.inspect('notes', 'from-reader')).pending === false);
  await t2.sim.setLink({ state: 'offline', rtt_ms: 80, relay: true }); // vom Leser an den Leiter weitergeleitet
  await until(() => a2.metrics.get().link.state === 'offline');
  assert.equal(a2.metrics.get().link.relay, true);
  await t1.sim.setLink({ state: 'online', relay: false });
  // verlorene Broadcasts: Leser holt per meta.local_seq nach
  t2.sim.dropBroadcasts(true);
  await a1.put('notes', { id: 'lost', v: 2 }).result;
  await tick(20);
  assert.equal(a2.get('notes', 'lost').get(), null);
  t2.sim.dropBroadcasts(false);
  assert.ok((await t2.sim.reconcile()) >= 1);
  assert.equal(a2.get('notes', 'lost').get().v, 2);
  // Leiterwechsel: t1 schließt, t2 übernimmt Outbox idempotent
  await t1.sim.setLink({ state: 'offline' });
  const pend = a2.put('notes', { id: 'after', v: 3 });
  await pend.local;
  await t1.close();
  await until(() => t2.isLeader);
  assert.ok(createShellKernel(t2).leader.get().leader_gen >= 2);
  assert.equal((await pend.result).status, 'accepted');
  await t2.close();
});

test('leader_gen: veraltete Leiteroperation wird verworfen, Neuwahl sendet erneut', async () => {
  const k = await mk({ latencyMs: 20 });
  const api = k.appApi('notes', '1');
  const h = api.put('notes', { id: 'g', v: 1 });
  await h.local;
  await k.sim.bumpLeaderGen(); // ein anderer Leiter hat die Generation erhöht
  const r = await h.result;
  assert.equal(r.status, 'accepted');
  assert.equal(k.sim.staleDiscards, 1);
  assert.equal(k.isLeader, true);
  await k.close();
});

test('Abmelden schließt DB und sperrt Zugriff; Outbox wird bei erneuter Anmeldung derselben Identität gesendet', async () => {
  const opts = { instanceId: `logout-${process.pid}`, userId: 'bob', latencyMs: 5 };
  const k1 = createStubKernel(opts);
  await k1.ready;
  await k1.sim.setLink({ state: 'offline' });
  const api = k1.appApi('notes', '1');
  await api.put('notes', { id: 'o', v: 1 }).local;
  await createShellKernel(k1).identity.logout();
  await assert.rejects(api.put('notes', { id: 'p' }).local, (e) => e.code === 'unavailable');
  assert.throws(()=>api.query('notes',{}).get(),e=>e.code==='unavailable');
  const other = createStubKernel({locks:testLocks, ...opts, userId: 'eve' });
  await other.ready;
  assert.equal((await other.sim.outbox()).length, 0, 'nie unter anderer Identität');
  await other.close();
  const k2 = createStubKernel(opts);
  await k2.ready;
  await k2.sim.settle();
  assert.equal((await k2.sim.inspect('notes', 'o')).pending, false);
  const k3 = await createShellKernel(k2).identity.switchInstance({ instanceId: `logout2-${process.pid}` });
  await k3.ready;
  assert.equal(k3.dbName, `ctox-logout2-${process.pid}-bob`);
  await k3.logout({ deleteData: true });
});

test('Kompatibilität zu 66edb98: veraltete Aliase zeigen auf die neuen Objekte', async () => {
  const k = await mk();
  assert.equal(k.statusZone, k.status);
  assert.equal(k.shell.statusZone, k.status);
  assert.deepEqual(await k.sim.windowState(), await k.sim.windowStateRows());
  await k.close();
});
