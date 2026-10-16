// v4-Fassung (App-API 4.0): mount(root, api, {restored}) hängt nur an Live-Abos der Plattform-Collections. Die Regeln
// der v3-Fassung bleiben: mount wartet auf keine Synchronisation, ein verweigerter Katalog-Lesezugriff ist sichtbar
// (v3: abgelehnter mount; 4.0: Zustandszone, damit die Shell nicht auf eine alte Version zurückfällt, §16.10),
// und Abos werden beim Schließen abgemeldet. Echte Chromium-Abnahme: tests/v4-api.test.mjs.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const moduleSource = readFileSync(
  process.env.CTOX_APP_STORE_MOUNT_SOURCE || new URL('./index.js', import.meta.url), 'utf8',
);
const mountSource = moduleSource.slice(
  moduleSource.indexOf('export async function mount(root, api'),
  moduleSource.indexOf('\nfunction restoreGrammarControls('),
).replace(/^export /, '');
assert.match(mountSource, /^async function mount\(root, api/);

function fixture({ forbidden = [] } = {}) {
  const calls = [];
  const reports = [];
  const subs = new Map();
  const state = { zone: new Set(), t: (key, fallback) => fallback ?? key };
  const api = {
    actor: { id: 'u1', role: 'admin', permissions: { get: () => [], subscribe: (cb) => { cb([]); return () => {}; } } },
    shell: { setTitle(title) { calls.push(`title:${title}`); }, locale: { get: () => 'de', subscribe: () => () => {} } },
    status: { report(item) { reports.push(item); }, clear() {} },
    query(name) {
      if (forbidden.includes(name)) { const error = new Error(`${name} forbidden`); error.code = 'forbidden'; throw error; }
      return { subscribe(cb) { subs.set(name, (subs.get(name) || 0) + 1); cb([{ id: `${name}-row` }]); return () => subs.set(name, subs.get(name) - 1); } };
    },
  };
  const context = vm.createContext({
    state, els: {}, Promise, clearTimeout, setTimeout,
    retireShelf() {}, applyLocale() {}, readLocale: () => 'de', applyRestored() {},
    loadModuleMarkup: async () => '<div>Store</div>', applyTranslations() {}, bindElements() {},
    applyHeaderActionIcons() {}, wireEvents() {}, autoWirePaneGrammar() {},
    rebuildCatalog() { calls.push(`catalog:${state.platformApps.length}/${state.platformVersions.length}`); },
    render() { calls.push('render'); }, restoreGrammarControls() {}, scheduleRender() { calls.push('schedule'); },
    reportStatus(level, code, text) { reports.push({ level, code, text }); state.zone.add(code); },
  });
  vm.runInContext(mountSource + '\nglobalThis.actualMount = mount;', context);
  const root = { dataset: {}, innerHTML: '', replaceChildren() { this.innerHTML = ''; this.cleared = true; } };
  return { mount: (r = root) => context.actualMount(r, api, {}), root, state, calls, reports, subs };
}

test('mount kommt sofort zurück und abonniert genau die vier Plattform-Collections', async () => {
  const f = fixture();
  const handle = await f.mount();
  assert.equal(f.root.dataset.app, 'app-store');
  assert.deepEqual([...f.subs.entries()], [['business_apps', 1], ['business_app_versions', 1], ['app_catalog', 1], ['app_drafts', 1]]);
  assert.ok(f.calls.includes('catalog:1/1'), 'Katalog aus dem ersten Live-Stand gebaut');
  assert.ok(f.calls.includes('render'));
  handle.unmount();
  assert.deepEqual([...f.subs.values()], [0, 0, 0, 0]);
  assert.equal(f.root.cleared, true);
});

test('verweigerter Katalog-Lesezugriff steht in der Zustandszone, mount löst trotzdem auf', async () => {
  const f = fixture({ forbidden: ['business_app_versions'] });
  const handle = await f.mount();
  assert.equal(f.reports.length, 1);
  assert.equal(f.reports[0].level, 'error');
  assert.equal(f.reports[0].code, 'app-store.data.business_app_versions');
  assert.ok(f.calls.includes('render'));
  handle.unmount();
  assert.deepEqual([...f.subs.values()], [0, 0, 0]);
});

test('eine abgelöste Montage räumt beim Schließen nicht die neue ab', async () => {
  const f = fixture();
  const firstRoot = { dataset: {}, replaceChildren() { this.cleared = true; } };
  const secondRoot = { dataset: {}, replaceChildren() { this.cleared = true; } };
  const first = await f.mount(firstRoot);
  const second = await f.mount(secondRoot);
  first.unmount();
  assert.equal(firstRoot.cleared, true);
  assert.equal(f.state.root, secondRoot, 'Zustand gehört weiter der neuen Montage');
  assert.deepEqual([...f.subs.values()], [1, 1, 1, 1]);
  second.unmount();
  assert.deepEqual([...f.subs.values()], [0, 0, 0, 0]);
  assert.equal(f.state.root, null);
});
