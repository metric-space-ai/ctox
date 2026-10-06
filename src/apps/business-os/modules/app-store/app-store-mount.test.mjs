import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const moduleSource = readFileSync(
  process.env.CTOX_APP_STORE_MOUNT_SOURCE || new URL('./index.js', import.meta.url), 'utf8',
);
const mountSource = moduleSource.slice(
  moduleSource.indexOf('export async function mount(ctx) {'),
  moduleSource.indexOf('\nfunction ensureStylesheet()'),
).replace(/^export /, '');
assert.match(mountSource, /^async function mount/);

function deferred() {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
}

function fixture(startCollection, loadError = null) {
  const calls = [];
  const warnings = [];
  let unsubscribed = 0;
  const state = {};
  const catalog = { modules: [{ id: 'mail', title: 'Mail' }] };
  const collection = {
    findOne(id) {
      assert.equal(id, 'module-catalog');
      return {
        async exec() { calls.push('catalog-read'); if (loadError) throw loadError; return catalog; },
        $: { subscribe() { calls.push('catalog-subscribe'); return { unsubscribe() { unsubscribed++; } }; } },
      };
    },
  };
  const ctx = { host: {}, sync: { startCollection }, db: { collection(name) {
    assert.equal(name, 'business_module_catalog'); return collection;
  } } };
  const context = vm.createContext({
    state, Promise, console: { warn(...args) { warnings.push(args); } },
    loadModuleMessages: async () => ({}), loadModuleMarkup: async () => '<div>Store</div>',
    applyTranslations() {}, ensureStylesheet() {}, bindElements() {}, applyHeaderActionIcons() {}, wireEvents() {},
    async loadCatalog() { state.catalog = await ctx.db.collection('business_module_catalog').findOne('module-catalog').exec(); },
    applyCatalogMarketplaceState() {}, render() { calls.push('render'); },
    mergeShellModulesIntoCatalog: (value) => value, normalizeMarketplace: (value) => value,
  });
  // Execute the actual exported mount body, retaining its await/rejection and
  // teardown behavior. DOM presentation is stubbed; this is not tenant E2E.
  vm.runInContext(mountSource.replace('import.meta.url', "'https://fixture.invalid/index.js'") + '\nglobalThis.actualMount = mount;', context);
  return { mount: () => context.actualMount(ctx), state, catalog, calls, warnings, unsubscribed: () => unsubscribed };
}

async function boundedSettlement(promise) {
  let timer;
  try {
    return await Promise.race([
      promise.then((value) => ({ status: 'resolved', value }), (error) => ({ status: 'rejected', error })),
      new Promise((resolve) => { timer = setTimeout(() => resolve({ status: 'pending' }), 100); }),
    ]);
  } finally { clearTimeout(timer); }
}

test('cached catalog mounts while native bridge warmup remains pending', async () => {
  const bridge = deferred();
  const starts = [];
  const f = fixture((name) => { starts.push(name); return bridge.promise; });
  const mounting = f.mount();
  try {
    const result = await boundedSettlement(mounting);
    assert.equal(result.status, 'resolved', 'native registration must not delay a valid cached catalog');
    assert.deepEqual(starts.sort(), ['business_commands', 'business_module_catalog']);
    assert.equal(f.state.catalog, f.catalog);
    assert.ok(f.calls.includes('render'));
    result.value();
    assert.equal(f.unsubscribed(), 1);
  } finally { bridge.resolve(); await mounting.catch(() => {}); }
});

test('sync warmup failure leaves an authorized cached catalog available', async () => {
  const f = fixture(() => { const error = new Error('native peer unavailable'); error.code = 'peer_connect_timeout'; throw error; });
  const result = await boundedSettlement(f.mount());
  assert.equal(result.status, 'resolved');
  assert.equal(f.state.catalog, f.catalog);
  assert.ok(f.calls.includes('catalog-read'));
  assert.ok(f.calls.includes('render'));
  await Promise.resolve();
  assert.equal(f.warnings.length, 1, 'warmup rejection is handled and observable');
  result.value();
});

test('a rejected catalog authority read still rejects the mount', async () => {
  const denied = new Error('catalog read forbidden'); denied.code = 'READ_PERMISSION_DENIED';
  const f = fixture(async () => ({}), denied);
  const result = await boundedSettlement(f.mount());
  assert.equal(result.status, 'rejected');
  assert.equal(result.error, denied);
  assert.equal(f.calls.includes('render'), false);
  assert.equal(f.calls.includes('catalog-subscribe'), false);
});
