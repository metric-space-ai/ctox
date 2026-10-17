// v4-Fassung: Regal-Lebenszyklus über mount(root, api) statt mount(ctx). Regel unverändert: Schließen räumt den
// Renderer ab, Wiederöffnen malt denselben Katalog neu, ein spät fertiger oder fehlgeschlagener Import einer
// abgelösten Montage darf die neue weder überschreiben noch als „Regal nicht verfügbar“ markieren.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const source = readFileSync(process.env.CTOX_APP_STORE_MOUNT_SOURCE || new URL('./index.js', import.meta.url), 'utf8');
const slice = (from, to) => {
  const start = source.indexOf(from);
  const end = source.indexOf(to, start);
  assert.ok(start >= 0 && end > start, `${from} … ${to}`);
  return source.slice(start, end);
};
const mount = slice('export async function mount(root, api', '\nfunction restoreGrammarControls(').replace(/^export /, '');
const retire = slice('function retireShelf() {', '\nasync function loadModuleMarkup(');
const shelf = slice('async function ensureShelf() {', '\nfunction renderCatalogBody(')
  .replace(/import\(['"]\.\/vendor\/store-shelf\/store-shelf\.mjs(?:\?[^'"]+)?['"]\)/, 'loadShelfModule()');
const code = [mount, retire, shelf].join('\n').replaceAll('import.meta.url', "'https://fixture.invalid/modules/app-store/index.js'");

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function fixture(loadShelfModule) {
  const instances = [], subscriptions = [], pending = [];
  const state = { zone: new Set(), t: (key, fallback) => fallback ?? key, shelf: null, shelfSignature: '', shelfUnavailable: false, drawerOpen: false, selectedId: '' };
  const els = {};
  const factory = {
    createStoreShelf(canvas, options) {
      const instance = {
        canvas, options, destroyed: false,
        setApps(apps) { assert.equal(this.destroyed, false); canvas.apps = apps; },
        select() {}, deselect() {},
        destroy() { this.destroyed = true; canvas.apps = []; },
      };
      instances.push(instance);
      return instance;
    },
  };
  let apps = [{ id: 'mail', title: 'Mail', kind: 'installed', category: 'Business' }];
  let context;
  context = vm.createContext({
    state, els, Promise, URL, setTimeout, clearTimeout, console: { warn() {}, error() {} },
    loadShelfModule: () => loadShelfModule(factory),
    applyLocale() {}, readLocale: () => 'de', applyRestored() {},
    loadModuleMarkup: async () => '<div>Store</div>', applyTranslations() {}, applyHeaderActionIcons() {}, wireEvents() {},
    autoWirePaneGrammar() {}, restoreGrammarControls() {}, reportStatus() {}, scheduleRender() {},
    bindElements(root) { for (const key of Object.keys(els)) delete els[key]; Object.assign(els, root.elements); },
    rebuildCatalog() {}, isInstalledCatalogItem: () => true, previewUrlFor: () => '',
    render() { if (!state.shelfUnavailable) pending.push(context.syncShelf(apps)); },
  });
  vm.runInContext(code + '\nglobalThis.actualMount = mount;', context);
  const api = {
    actor: { id: 'u1', role: 'admin', permissions: { subscribe: () => () => {} } },
    shell: { setTitle() {}, locale: { subscribe: () => () => {} } },
    status: { report() {}, clear() {} },
    query(name) {
      return { subscribe(callback) {
        const subscription = { name, callback, disposed: false };
        subscriptions.push(subscription);
        callback([]);
        return () => { subscription.disposed = true; };
      } };
    },
  };
  function host(id) {
    return { id, dataset: {}, innerHTML: '', replaceChildren() {}, elements: {
      shelfCanvas: { id, apps: [] }, shelfStage: {}, shelfScroll: { clientHeight: 600 },
      shelfTrack: { style: {} }, shelfHint: { hidden: false },
    } };
  }
  return { state, instances, subscriptions, host, setApps: (next) => { apps = next; },
    mount: (root) => context.actualMount(root, api, {}),
    flush: () => Promise.all(pending), ids: (root) => Array.from(root.elements.shelfCanvas.apps, (app) => app.id) };
}

test('closing retires the renderer and reopening paints the same cached catalogue on the new canvas', async () => {
  const f = fixture((factory) => Promise.resolve(factory));
  const first = f.host('first');
  const closeFirst = await f.mount(first); await f.flush();
  assert.deepEqual(f.ids(first), ['mail']);
  closeFirst.unmount();
  assert.equal(f.instances[0].destroyed, true);
  assert.deepEqual(f.ids(first), []);
  const second = f.host('second');
  const closeSecond = await f.mount(second); await f.flush();
  assert.deepEqual(f.ids(second), ['mail'], 'unchanged catalogue must populate a fresh renderer');
  assert.equal(f.instances.length, 2);
  assert.equal(f.instances[1].canvas, second.elements.shelfCanvas);
  closeFirst.unmount();
  assert.equal(f.instances[1].destroyed, false, 'old cleanup cannot retire the reopened window');
  closeSecond.unmount();
  assert.equal(f.instances[1].destroyed, true);
  assert.ok(f.subscriptions.every((subscription) => subscription.disposed));
});

test('an import finishing after close cannot create a renderer or overwrite a newer mount', async () => {
  const late = deferred(); let loads = 0, factory;
  const f = fixture((value) => { factory = value; return ++loads === 1 ? late.promise : Promise.resolve(value); });
  const first = f.host('retired');
  const closeFirst = await f.mount(first); closeFirst.unmount();
  const second = f.host('current');
  const closeSecond = await f.mount(second);
  late.resolve(factory); await f.flush();
  assert.equal(f.instances.length, 1);
  assert.equal(f.instances[0].canvas, second.elements.shelfCanvas);
  assert.deepEqual(f.ids(first), []);
  assert.deepEqual(f.ids(second), ['mail']);
  closeSecond.unmount();
});

test('a retired import rejection cannot switch the reopened shelf to DOM cards', async () => {
  const late = deferred(); let loads = 0;
  const f = fixture((factory) => (++loads === 1 ? late.promise : Promise.resolve(factory)));
  const closeFirst = await f.mount(f.host('retired')); closeFirst.unmount();
  const second = f.host('current');
  const closeSecond = await f.mount(second);
  late.reject(new Error('retired import failed')); await f.flush();
  assert.equal(f.state.shelfUnavailable, false);
  assert.deepEqual(f.ids(second), ['mail']);
  closeSecond.unmount();
});
