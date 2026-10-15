// K-1b: Shell-API-Lücken aus der Shell-v4-Anbindung (§16.19 Nr. 2/3/6, §16.22 A6/A9, §16.26, Befund B4, Binärartefakte).
// Je Punkt ein Test; Mutationsbeweise in ~/.local/state/ctox-sync-v4/docs/befund-k-1b.md.
import { testLocks } from './web-locks.mjs';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createStubKernel, createShellKernel } from '../kernel.mjs';

let n = 0;
async function mk(o = {}) {
  const k = createStubKernel({ locks: testLocks, instanceId: `k1b${process.pid}-${++n}`, userId: 'u1', latencyMs: 5, ...o });
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

test('1 §16.19 Nr. 2: activation.reportFailed → failed für diesen Client, Zeiger auf letzte funktionierende Version, app_activation failed', async () => {
  const k = await mk();
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

test('2 §16.19 Nr. 3: windowState.get/put/del je Tab und Nutzer (IDB window_state), überlebt Neuladen desselben Tabs', async () => {
  const instanceId = `k1b-ws-${process.pid}`;
  const a = await mk({ instanceId, tabKey: 'tab-A' });
  assert.equal(await a.windowState.get('layout'), null);
  await a.windowState.put('layout', { open: ['notes'], focus: 'notes' });
  assert.deepEqual(await a.windowState.get('layout'), { open: ['notes'], focus: 'notes' });
  const row = (await a.sim.windowStateRows()).find((r) => r.key.endsWith(':layout'));
  assert.deepEqual([row.tab_key, row.user_id], ['tab-A', 'u1']);
  await assert.rejects(a.windowState.put('', 1), (e) => e.code === 'invalid');
  await a.close();
  const other = await mk({ instanceId, tabKey: 'tab-B' }); // anderer Tab, gleicher Nutzer
  assert.equal(await other.windowState.get('layout'), null, 'Fensterzustand ist je Tab');
  await other.close();
  const reload = await mk({ instanceId, tabKey: 'tab-A' }); // derselbe Tab nach Neuladen
  assert.deepEqual(await reload.windowState.get('layout'), { open: ['notes'], focus: 'notes' });
  await reload.windowState.del('layout');
  assert.equal(await reload.windowState.get('layout'), null);
  await reload.close();
});

test('3 §16.19 Nr. 6: Manifest mit layout (9 Felder) und styles; kernel.apps-Einträge liefern beides', async () => {
  const k = await mk();
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

test('4a §16.22 A6/A9: draft.setCollections(vollständiges Schema) → Diff kompatibel/neu/Generation; Generation ohne Migration → invalid', async () => {
  const k = await mk();
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

test('4b §16.26 + Befund B4: kein manifest.json im Entwurf, fehlende Verweise → invalid, publish erbt alle Basis-Manifest-Felder', async () => {
  const k = await mk();
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

test('5 §16.19 Nr. 3: shell_preferences ist Plattform-Collection (Schema ctox-platform), nur die eigene Zeile', async () => {
  const k = await mk();
  await k.sim.publishAppVersion('shell', '1', { files: { 'index.js': MOUNT } });
  const api = k.appApi('shell', '1');
  const own = await api.put('shell_preferences', { id: 'u1', user_id: 'u1', pins: ['crm'], launcher_order: ['crm', 'notes'] }).result;
  assert.equal(own.status, 'accepted', JSON.stringify(own));
  await assert.rejects(api.put('shell_preferences', { id: 'u2', pins: [] }).local, (e) => e.code === 'forbidden');
  await assert.rejects(api.put('shell_preferences', { id: 'u1', fremd: 1 }).local, (e) => e.code === 'invalid', 'Schema aus dem Plattform-Manifest');
  // Zeile eines anderen Nutzers im Spiegel (Rollenwechsel im selben Store) bleibt unsichtbar
  k.sim.setActor({ id: 'u2' });
  assert.equal((await api.put('shell_preferences', { id: 'u2', pins: ['x'] }).result).status, 'accepted');
  k.sim.setActor({ id: 'u1' });
  const q = api.query('shell_preferences', { selector: {} });
  await until(() => q.get().length === 1);
  assert.deepEqual(q.get().map((d) => [d.id, d.pins]), [['u1', ['crm']]]);
  assert.equal(api.get('shell_preferences', 'u2').get(), null);
  await k.close();
});

test('6 Binärartefakte: Uint8Array/Blob/ArrayBuffer, Hash über die Bytes, PNG als image/png lesbar', async () => {
  const k = await mk();
  // 1×1-PNG mit Bytes > 0x7f (würde über String()/UTF-8 verfälscht)
  const png = Uint8Array.from(Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==', 'base64'));
  await k.sim.publishAppVersion('crm', '1', {
    files: { 'index.js': MOUNT, 'icon.png': png, 'icon2.png': new Blob([png]), 'icon3.png': png.buffer.slice(0) },
    layout: { icon_asset: 'icon.png' },
  });
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
