import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const source = readFileSync(process.env.CTOX_APP_STORE_MOUNT_SOURCE || new URL('./index.js', import.meta.url), 'utf8');
const mount = source.slice(source.indexOf('export async function mount(ctx) {'), source.indexOf('\nfunction ensureStylesheet()')).replace(/^export /, '');
const shelf = source.slice(source.indexOf('async function ensureShelf() {'), source.indexOf('\nfunction renderCatalogBody('))
  .replace("import('../../vendor/store-shelf/store-shelf.mjs')", 'loadShelfModule()');
const code = (mount + '\n' + shelf).replaceAll('import.meta.url', "'https://fixture.invalid/modules/app-store/index.js'");

function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function fixture(loadShelfModule) {
  const instances = [], subscriptions = [], renders = [], pending = [];
  const state = { ctx: null, catalog: null, shelf: null, shelfSignature: '', shelfUnavailable: false };
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
  const catalog = { modules: [{ id: 'mail', title: 'Mail', kind: 'installed', category: 'Business' }] };
  let context;
  context = vm.createContext({
    state, els, Promise, URL, setTimeout, clearTimeout, console: { warn() {}, error() {} },
    loadShelfModule: () => loadShelfModule(factory),
    loadModuleMessages: async () => ({}), loadModuleMarkup: async () => '<div>Store</div>',
    applyTranslations() {}, ensureStylesheet() {}, applyHeaderActionIcons() {}, wireEvents() {},
    bindElements(host) { Object.assign(els, host.elements); },
    async loadCatalog() { state.catalog = catalog; },
    applyCatalogMarketplaceState() {}, mergeShellModulesIntoCatalog: value => value,
    normalizeMarketplace: value => value,
    isInstalledCatalogItem: () => true, previewUrlFor: () => '',
    render() { renders.push(state.ctx.host); if (!state.shelfUnavailable) pending.push(context.syncShelf(state.catalog.modules)); },
  });
  vm.runInContext(code + '\nglobalThis.actualMount = mount;', context);
  function host(id) {
    return { id, innerHTML: '', elements: {
      shelfCanvas: { id, apps: [] }, shelfStage: {}, shelfScroll: { clientHeight: 600 },
      shelfTrack: { style: {} }, shelfHint: { hidden: false },
    } };
  }
  function ctx(host) {
    return { host, locale: 'de', sync: { startCollection: () => new Promise(() => {}) },
      db: { collection: () => ({ findOne: () => ({ $: {
        subscribe(callback) {
          const subscription = { callback, disposed: false, unsubscribe() { this.disposed = true; } };
          subscriptions.push(subscription); return subscription;
        },
      } }) }) },
    };
  }
  return { state, instances, subscriptions, renders, host, mount: host => context.actualMount(ctx(host)),
    flush: () => Promise.all(pending), ids: host => Array.from(host.elements.shelfCanvas.apps, app => app.id) };
}

test('closing retires the renderer and reopening paints the same cached catalogue on the new canvas', async () => {
  const f = fixture(factory => Promise.resolve(factory));
  const first = f.host('first');
  const closeFirst = await f.mount(first); await f.flush();
  assert.deepEqual(f.ids(first), ['mail']);
  closeFirst();
  assert.equal(f.instances[0].destroyed, true);
  assert.deepEqual(f.ids(first), []);
  const second = f.host('second');
  const closeSecond = await f.mount(second); await f.flush();
  assert.deepEqual(f.ids(second), ['mail'], 'unchanged catalogue must populate a fresh renderer');
  assert.equal(f.instances.length, 2);
  assert.equal(f.instances[1].canvas, second.elements.shelfCanvas);
  closeFirst();
  assert.equal(f.instances[1].destroyed, false, 'old cleanup cannot retire the reopened window');
  f.subscriptions[0].callback({ toJSON: () => ({ modules: [] }) });
  assert.deepEqual(f.ids(second), ['mail']);
  closeSecond();
  assert.equal(f.instances[1].destroyed, true);
  assert.ok(f.subscriptions.every(subscription => subscription.disposed));
});

test('an import finishing after close cannot create a renderer or overwrite a newer mount', async () => {
  const late = deferred(); let loads = 0, factory;
  const f = fixture(value => { factory = value; return ++loads === 1 ? late.promise : Promise.resolve(value); });
  const first = f.host('retired');
  const closeFirst = await f.mount(first); closeFirst();
  const second = f.host('current');
  const closeSecond = await f.mount(second);
  late.resolve(factory); await f.flush();
  assert.equal(f.instances.length, 1);
  assert.equal(f.instances[0].canvas, second.elements.shelfCanvas);
  assert.deepEqual(f.ids(first), []);
  assert.deepEqual(f.ids(second), ['mail']);
  closeSecond();
});

test('a retired import rejection cannot mark the reopened shelf unavailable', async () => {
  const late = deferred(); let loads = 0;
  const f = fixture(factory => ++loads === 1 ? late.promise : Promise.resolve(factory));
  const closeFirst = await f.mount(f.host('retired')); closeFirst();
  const second = f.host('current');
  const closeSecond = await f.mount(second);
  late.reject(new Error('retired import failed')); await f.flush();
  assert.equal(f.state.shelfUnavailable, false);
  assert.deepEqual(f.ids(second), ['mail']);
  closeSecond();
});
