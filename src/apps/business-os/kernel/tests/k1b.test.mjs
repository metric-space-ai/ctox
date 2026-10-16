// K-1b: Shell-API-Lücken aus der Shell-v4-Anbindung (§16.19 Nr. 2/3/6, §16.22 A6/A9, §16.26, Befund B4, Binärartefakte).
// Je Punkt ein Test; Mutationsbeweise in ~/.local/state/ctox-sync-v4/docs/befund-k-1b.md.
import { testLocks } from './web-locks.mjs';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createStubKernel, createShellKernel } from '../kernel.mjs';

let n = 0;
// Jeder Kernel wird auch bei einem roten assert geschlossen: ein offener Kernel hält seinen BroadcastChannel
// (MessagePort) und damit den Testprozess am Leben – der Test war dann rot, der Lauf endete aber nie (K-1c Punkt 0).
async function mk(t, o = {}) {
  const k = createStubKernel({ locks: testLocks, instanceId: `k1b${process.pid}-${++n}`, userId: 'u1', latencyMs: 5, ...o });
  t.after(() => k.close());
  await k.ready;
  return k;
}
const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms));
async function until(pred, ms = 3000) {
  const t0 = Date.now();
  while (!(await pred())) { if (Date.now() - t0 > ms) throw new Error('until: Zeitüberschreitung'); await tick(5); }
}
const MOUNT = 'export function mount(){ return { unmount(){} }; }';
const sha = (bytes) => createHash('sha256').update(bytes).digest('hex');

test('1 §16.19 Nr. 2: activation.reportFailed → failed für diesen Client, Zeiger auf letzte funktionierende Version, app_activation failed', async (t) => {
  const k = await mk(t);
  const sk = createShellKernel(k);
  await k.sim.publishAppVersion('notes', '1', { files: { 'index.js': MOUNT } });
  await k.sim.publishAppVersion('notes', '2', { files: { 'index.js': `${MOUNT}\n// v2` } });
  assert.equal(sk.activation.get('notes').get().activated_ver, '2');
  assert.equal(k.activation, sk.activation, 'kernel.activation = kernel.shell.activation');
  const v = await k.activation.reportFailed('notes', '2', 'Shell: mount wirft');
  assert.deepEqual([v.state, v.target_ver, v.activated_ver], ['failed', '2', '1']);
  const live = sk.activation.get('notes').get();
  assert.deepEqual([live.state, live.activated_ver, live.error], ['failed', '1', 'reported: Shell: mount wirft']);
  assert.equal(sk.apps.get()[0].activated_ver, '1', 'kernel.apps zeigt die letzte funktionierende Version');
  const last = k.sim.activationReports.at(-1);
  assert.deepEqual([last.app_id, last.app_ver, last.state], ['notes', '2', 'failed'], 'app_activation{state:failed} an den Core');
  await assert.rejects(k.activation.reportFailed('', '2'), (e) => e.code === 'invalid');
  await k.close();
});

test('2 §16.19 Nr. 3: windowState.get/put/del je Tab und Nutzer (IDB window_state), überlebt Neuladen desselben Tabs', async (t) => {
  const instanceId = `k1b-ws-${process.pid}`;
  const a = await mk(t, { instanceId, tabKey: 'tab-A' });
  assert.equal(await a.windowState.get('layout'), null);
  await a.windowState.put('layout', { open: ['notes'], focus: 'notes' });
  assert.deepEqual(await a.windowState.get('layout'), { open: ['notes'], focus: 'notes' });
  const row = (await a.sim.windowStateRows()).find((r) => r.key.endsWith(':layout'));
  assert.deepEqual([row.tab_key, row.user_id], ['tab-A', 'u1']);
  await assert.rejects(a.windowState.put('', 1), (e) => e.code === 'invalid');
  await a.close();
  const other = await mk(t, { instanceId, tabKey: 'tab-B' }); // anderer Tab, gleicher Nutzer
  assert.equal(await other.windowState.get('layout'), null, 'Fensterzustand ist je Tab');
  await other.close();
  const reload = await mk(t, { instanceId, tabKey: 'tab-A' }); // derselbe Tab nach Neuladen
  assert.deepEqual(await reload.windowState.get('layout'), { open: ['notes'], focus: 'notes' });
  await reload.windowState.del('layout');
  assert.equal(await reload.windowState.get('layout'), null);
  await reload.close();
});

test('3 §16.19 Nr. 6: Manifest mit layout (9 Felder) und styles; kernel.apps-Einträge liefern beides', async (t) => {
  const k = await mk(t);
  await k.sim.publishAppVersion('crm', '1', {
    files: { 'index.js': MOUNT, 'styles/app.css': 'p{}' }, title: 'CRM',
    layout: { icon_asset: 'icon.png', category: 'sales', default_width: 960, presentation: 'window' }, styles: ['styles/app.css'],
  });
  const e = k.apps.get()[0];
  assert.deepEqual(e.layout, {
    icon_asset: 'icon.png', icon_svg: null, category: 'sales', frame_palette: null, default_width: 960, default_height: null,
    min_width: null, min_height: null, presentation: 'window',
  });
  assert.deepEqual(e.styles, ['styles/app.css']);
  const m = await k.artifacts.manifest('crm', '1');
  assert.equal(Object.keys(m.layout).length, 9);
  assert.deepEqual(m.styles, ['styles/app.css']);
  await k.close();
});

test('4a §16.22 A6/A9: draft.setCollections(vollständiges Schema) → Diff kompatibel/neu/Generation; Generation ohne Migration → invalid', async (t) => {
  const k = await mk(t);
  const leads = { schema: { primaryKey: 'id', type: 'object', properties: { id: { type: 'string' }, name: { type: 'string' } }, required: ['id'] } };
  await k.sim.publishAppVersion('crm', '1', { files: { 'index.js': MOUNT }, collections: { leads } });
  const src = k.source('crm');
  const withProps = (props, extra = {}) => ({ collections: { leads: { schema: { ...leads.schema, properties: { ...leads.schema.properties, ...props } } }, ...extra } });
  // optionales Feld mit Default → kompatibel; neue Collection → new
  const r1 = await src.draft.setCollections(JSON.stringify(withProps({ score: { type: 'number', default: 0 } }, { notes: { schema: { primaryKey: 'id', type: 'object', properties: { id: { type: 'string' } } } } })));
  assert.equal(r1.base_app_ver, '1');
  assert.deepEqual(r1.diff.map((x) => [x.name, x.kind]), [['leads', 'compatible'], ['notes', 'new']]);
  // Typwechsel → neue Generation, ohne Migration nicht veröffentlichbar
  const r2 = await src.draft.setCollections(withProps({ name: { type: 'number' } }));
  assert.deepEqual(r2.diff.map((x) => [x.name, x.kind, x.needsMigration]), [['leads', 'generation', true]]);
  assert.match(r2.diff[0].reasons[0], /Typ von „name“: string → number/);
  const bad = await src.draft.publish().result;
  assert.deepEqual([bad.status, bad.reason, bad.detail], ['rejected', 'invalid', 'Migration fehlt: leads']);
  // mit Migration (from_ver = Basis) → angenommen, Generation leads@g2
  await src.draft.setCollections({
    ...withProps({ name: { type: 'number' } }), migrations: { leads: { from_ver: '1', steps: [{ op: 'cast', field: 'name', to: 'number' }] } },
  });
  const ok = await src.draft.publish().result;
  assert.equal(ok.status, 'accepted', JSON.stringify(ok));
  assert.deepEqual(ok.diff, [{ name: 'leads', kind: 'generation' }]);
  await until(() => k.apps.get()[0].activated_ver === ok.rev);
  assert.equal((await k.artifacts.manifest('crm', ok.rev)).generations.leads, 2);
  await assert.rejects(src.draft.setCollections('{kaputt'), (e) => e.code === 'invalid');
  await k.close();
});

test('4b §16.26 + Befund B4: kein manifest.json im Entwurf, fehlende Verweise → invalid, publish erbt alle Basis-Manifest-Felder', async (t) => {
  const k = await mk(t);
  const extra = {
    triggers: [{ handler: 'handlers/touch.js', on: { collection: 'leads', op: 'insert' } }], command_rights: { 'crm.*': { roles: ['user'] } },
    foreign_collections: { contacts: 'ctox-platform' }, exports: { leads: ['id'] }, row_filters: { leads: { owner: '$actor' } }, write_guard: { leads: 'own' },
  };
  await k.sim.publishAppVersion('crm', '1', { files: { 'index.js': MOUNT, 'manifest.json': '{"title":"alt"}' }, manifestExtra: extra });
  const src = k.source('crm');
  await src.draft.write('README.md', '# crm');
  assert.deepEqual(await src.list(), ['README.md', 'index.js'], 'manifest.json der Basis verworfen');
  await assert.rejects(src.draft.write('manifest.json', '{}'), (e) => e.code === 'invalid');
  await src.draft.setManifest({ handlers: { 'crm.touch': 'handlers/fehlt.js' } });
  const bad = await src.draft.publish().result;
  assert.deepEqual([bad.status, bad.reason, bad.detail], ['rejected', 'invalid', 'Verweis auf fehlende Datei: handlers.crm.touch → handlers/fehlt.js']);
  await src.draft.write('handlers/fehlt.js', 'export default async function handle() { return 1; }');
  await src.draft.setManifest({ exports: { leads: ['id', 'name'] } }); // im Entwurf gesetzt → geht vor
  const ok = await src.draft.publish().result;
  assert.equal(ok.status, 'accepted', JSON.stringify(ok));
  await until(() => k.apps.get()[0].activated_ver === ok.rev);
  const m = await k.artifacts.manifest('crm', ok.rev);
  for (const f of ['triggers', 'command_rights', 'foreign_collections', 'row_filters', 'write_guard']) assert.deepEqual(m[f], extra[f], f);
  assert.deepEqual(m.exports, { leads: ['id', 'name'] });
  assert.equal(m.files['manifest.json'], undefined);
  assert.equal(m.app_ver, ok.rev, 'berechnete Felder nicht geerbt');
  await k.close();
});

test('5 §16.19 Nr. 3: shell_preferences ist Plattform-Collection (Schema ctox-platform), nur die eigene Zeile', async (t) => {
  const k = await mk(t);
  await k.sim.publishAppVersion('shell', '1', { files: { 'index.js': MOUNT } });
  const api = k.appApi('shell', '1');
  const own = await api.put('shell_preferences', { id: 'u1', user_id: 'u1', pins: ['crm'], launcher_order: ['crm', 'notes'] }).result;
  assert.equal(own.status, 'accepted', JSON.stringify(own));
  await assert.rejects(api.put('shell_preferences', { id: 'u2', pins: [] }).local, (e) => e.code === 'forbidden');
  await assert.rejects(api.put('shell_preferences', { id: 'u1', fremd: 1 }).local, (e) => e.code === 'invalid', 'Schema aus dem Plattform-Manifest');
  // Zeile eines anderen Nutzers, die der Core liefert, bleibt unsichtbar
  await k.sim.push('shell_preferences', [{ id: 'u2', pins: ['x'] }]);
  const q = api.query('shell_preferences', { selector: {} });
  await until(() => q.get().length === 1);
  assert.deepEqual(q.get().map((d) => [d.id, d.pins]), [['u1', ['crm']]]);
  assert.equal(api.get('shell_preferences', 'u2').get(), null);
  await k.close();
});

test('6 Binärartefakte: Uint8Array/Blob/ArrayBuffer, Hash über die Bytes, PNG als image/png lesbar', async (t) => {
  const k = await mk(t);
  // 1×1-PNG mit Bytes > 0x7f (würde über String()/UTF-8 verfälscht)
  const png = Uint8Array.from(Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==', 'base64'));
  await k.sim.publishAppVersion('crm', '1', {
    files: { 'index.js': MOUNT, 'icon.png': png, 'icon2.png': new Blob([png]), 'icon3.png': png.buffer.slice(0) },
    layout: { icon_asset: 'icon.png' },
  });
  assert.equal(k.apps.get()[0].state, 'activated', 'alle Dateien gegen ihren sha256 geprüft');
  const m = await k.artifacts.manifest('crm', '1');
  for (const p of ['icon.png', 'icon2.png', 'icon3.png']) assert.equal(m.files[p], sha(png), `${p}: sha256 über die Bytes`);
  const blob = await k.artifacts.read(m.files['icon.png']);
  assert.equal(blob.type, 'image/png');
  assert.deepEqual(new Uint8Array(await blob.arrayBuffer()), png);
  // Entwurfsweg: PNG-Bytes bleiben Bytes bis zum publish
  const src = k.source('crm');
  await src.draft.write('icon-new.png', new Blob([png]));
  const r = await src.draft.publish().result;
  assert.equal(r.status, 'accepted', JSON.stringify(r));
  await until(() => k.apps.get()[0].activated_ver === r.rev);
  assert.equal((await k.artifacts.manifest('crm', r.rev)).files['icon-new.png'], sha(png));
  await k.close();
});

test('0 §16.15a Nr. 3: Hash-Fehlschlag beim Artefaktabruf → einmal neu holen, wiederholt → failed (deterministisch, kein Hängen)', async (t) => {
  const k = await mk(t);
  const png = Uint8Array.from([137, 80, 78, 71, 13, 10, 26, 10, 200, 255]);
  k.sim.corruptNextFetch(1);
  const ok = await k.sim.publishAppVersion('crm', '1', { files: { 'index.js': MOUNT, 'icon.png': png } });
  assert.deepEqual([ok.state, k.sim.refetches], ['activated', 1], 'ein verfälschter Abruf wird neu geholt');
  k.sim.corruptNextFetch(2);
  const t0 = Date.now();
  const bad = await k.sim.publishAppVersion('crm', '2', { files: { 'index.js': `${MOUNT}\n// v2`, 'icon.png': Uint8Array.from([...png, 1]) } });
  assert.deepEqual([bad.state, bad.target_ver, bad.activated_ver], ['failed', '2', '1']);
  assert.match(bad.error, /^hash_mismatch:/);
  assert.ok(Date.now() - t0 < 5000, 'failed ohne Warten auf ein Ereignis, das nie kommt');
  assert.equal(k.apps.get()[0].activated_ver, '1', 'letzte funktionierende Version läuft weiter');
});

test('K-1c 1 / K-1d §16.39: artifacts.url synchron ohne Merkfunktion, artifacts.bind friert die Bindung je Mount ein', async (t) => {
  const k = await mk(t);
  await k.sim.publishAppVersion('shell', '7', { files: { 'index.js': MOUNT } });
  await k.sim.publishAppVersion('crm', '3', { files: { 'index.js': MOUNT, 'img/a b.png': 'x' } });
  const url = k.artifacts.url('crm', '3', 'img/a b.png');
  assert.equal(typeof url, 'string', 'synchron, kein Promise');
  assert.equal(url, '/~a/crm@3~shell@7/img/a%20b.png');
  const mount = k.artifacts.bind('crm', '3');
  assert.deepEqual([mount.appId, mount.ver, mount.shellVer, mount.root, Object.isFrozen(mount)], ['crm', '3', '7', '/~a/crm@3~shell@7/', true]);
  await k.sim.publishAppVersion('shell', '8', { files: { 'index.js': `${MOUNT}\n// 8` } });
  assert.equal(mount.url('index.js'), '/~a/crm@3~shell@7/index.js', 'Mount-Bindung bleibt auf ihrer Shell-Version');
  assert.equal(k.artifacts.url('crm', '3', 'index.js'), '/~a/crm@3~shell@8/index.js', 'ohne Bindung: aktuell aktive Shell (neuer Mount)');
  assert.equal(k.artifacts.url('crm', '3', 'index.js', { shellVer: '7' }), '/~a/crm@3~shell@7/index.js');
  assert.equal(k.artifacts.url('shell', '8', 'index.js'), '/~a/shell@8/index.js');
  assert.throws(() => k.artifacts.url('crm', '3', '../x'), (e) => e.code === 'invalid');
  assert.throws(() => mount.url('../x'), (e) => e.code === 'invalid');
});

test('K-1c 2 §16.39 Nr. 2 + LIBS2: Lib-Bindung aus dem Shell-Manifest in der Instanz-Wurzel; SW-Miss holt genau die Minor der URL; lib-hit → LRU', async (t) => {
  const k = await mk(t, { libByteBudget: 25 });
  await k.sim.publishAppVersion('lib:monaco', '0.3', { files: { 'a.js': 'A'.repeat(10), 'b.js': 'B'.repeat(10), 'c.js': 'C'.repeat(10) } });
  await k.sim.publishAppVersion('lib:ctox-office', '1.4', { files: { 'o.js': 'o' } });
  assert.deepEqual(k.apps.get(), [], 'Bibliotheken sind keine Apps und werden nicht eager aktiviert');
  await k.sim.publishAppVersion('shell', '5', { files: { 'index.js': MOUNT }, manifestExtra: { libs: { monaco: '0', 'ctox-office': '1' } } });
  await k.sim.publishAppVersion('crm', '1', { files: { 'index.js': MOUNT } });
  const mount = k.artifacts.bind('crm', '1');
  assert.equal(mount.url('index.js'), '/~a/crm@1~shell@5~lib:ctox-office@1.4~lib:monaco@0.3/index.js');
  assert.equal(k.artifacts.url('shell', '5', 'index.js'), '/~a/shell@5~lib:ctox-office@1.4~lib:monaco@0.3/index.js');
  assert.equal(k.artifacts.url('lib:monaco', '0.3', 'a.js'), '/~a/lib:monaco@0.3/a.js');
  assert.throws(() => k.artifacts.url('lib:monaco', '0', 'x.js'), (e) => e.code === 'invalid');
  await k.sim.publishAppVersion('lib:monaco', '0.4', { files: { 'a.js': 'a'.repeat(10) } });
  assert.equal(mount.url('index.js'), '/~a/crm@1~shell@5~lib:ctox-office@1.4~lib:monaco@0.3/index.js', 'Mount bleibt auf 0.3');
  assert.equal(k.artifacts.url('crm', '1', 'index.js'), '/~a/crm@1~shell@5~lib:ctox-office@1.4~lib:monaco@0.4/index.js', 'neuer Mount: höchste Minor');
  await k.sim.settle();
  assert.equal((await k.sim.inspectMeta('active:lib:monaco@0')).value, '0.4');
  // SW-Miss mit alter Minor in der URL, obwohl 0.4 aktiv ist → genau die alte Datei
  const replies = [];
  const port = { postMessage: (m) => replies.push(m) };
  const lib = (path, ver = '0.3') => k.libs.onSwMessage({ type: 'ctox:lib-miss', lib: 'monaco', major: '0', path, ver, sha: null, reason: 'artifact' }, port);
  await lib('a.js');
  const m03 = await k.artifacts.manifest('lib:monaco', '0.3');
  const rowsNow = await k.sim.artifactRows();
  assert.equal(rowsNow.find((r) => r.app_id === 'lib:monaco')?.hash, m03.files['a.js'], 'Datei der Minor aus der URL, nicht der aktiven');
  assert.equal(rowsNow.find((r) => r.app_id === 'lib:monaco')?.content, 'A'.repeat(10));
  await lib('b.js');
  await lib('fehlt.js');
  await lib('a.js', null);
  assert.deepEqual(replies.map((m) => [m.type, m.reason ?? null]), [['ctox:lib-ready', null], ['ctox:lib-ready', null], ['ctox:lib-abort', 'not_found'], ['ctox:lib-abort', 'invalid']]);
  const libRows = async () => (await k.sim.artifactRows()).filter((r) => r.app_id === 'lib:monaco').map((r) => r.path).sort();
  assert.deepEqual(await libRows(), ['a.js', 'b.js']);
  // a.js wird getroffen (SW bündelt), danach c.js: Budget 25 Bytes → b.js (am längsten ungenutzt) fliegt
  await tick(5);
  await k.libs.onSwMessage({ type: 'ctox:lib-hit', hits: [{ lib: 'monaco', major: '0', ver: '0.3', path: 'a.js', sha: m03.files['a.js'], n: 2, last_ms: Date.now() + 1000 }] });
  await lib('c.js');
  assert.deepEqual(await libRows(), ['a.js', 'c.js']);
});

test('K-1c 3 §16.39 Nr. 4: Entwürfe in der Plattform-Collection app_drafts (id <user>/<app>, Dateien als sha256, über die Outbox zum Core)', async (t) => {
  const instanceId = `k1c-drafts-${process.pid}`;
  const k = await mk(t, { instanceId, tabKey: 'A' });
  await k.sim.publishAppVersion('crm', '1', { files: { 'index.js': MOUNT } });
  const src = k.source('crm');
  await src.draft.write('README.md', '# crm');
  await src.draft.write('icon.png', Uint8Array.from([137, 80, 78, 71, 255]));
  await src.draft.setManifest({ title: 'CRM' });
  await k.sim.settle();
  const row = await k.sim.inspect('app_drafts', 'u1/crm');
  assert.ok(row?.base?.seq && !row.pending, 'vom Core bestätigt (über die Outbox)');
  assert.deepEqual(Object.keys(row.data).sort(), ['app_id', 'base_app_ver', 'files', 'id', 'manifest_fields', 'updated_at_ms', 'user_id']);
  assert.deepEqual([row.data.user_id, row.data.base_app_ver, row.data.manifest_fields], ['u1', '1', { title: 'CRM' }]);
  assert.equal(row.data.files['README.md'], sha(Buffer.from('# crm')), 'Datei als sha256, Inhalt im CAS');
  assert.equal(await k.sim.inspectMeta('draft:crm'), undefined, 'nichts mehr in IDB meta');
  // Apps schreiben app_drafts nicht direkt
  await assert.rejects(k.appApi('crm', '1').put('app_drafts', { id: 'u1/crm', app_id: 'crm', user_id: 'u1' }).local, (e) => e.code === 'forbidden');
  await k.close();
  // Neustart desselben Nutzers: Entwurf samt Binärdatei wieder da
  const k2 = await mk(t, { instanceId, tabKey: 'A' });
  const info = await k2.source('crm').draft.info();
  assert.equal(info.files['README.md'], '# crm');
  assert.deepEqual([...info.files['icon.png']], [137, 80, 78, 71, 255]);
  const r = await k2.source('crm').draft.publish().result;
  assert.equal(r.status, 'accepted', JSON.stringify(r));
  await k2.sim.settle();
  assert.equal(await k2.source('crm').draft.info(), null, 'publish löscht den eigenen Entwurf');
  assert.equal((await k2.sim.inspect('app_drafts', 'u1/crm'))?.data ?? null, null);
});
