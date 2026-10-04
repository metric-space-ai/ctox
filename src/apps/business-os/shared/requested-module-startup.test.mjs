import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';

const source = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const definition = name => {
  const result = source.match(new RegExp('^async function ' + name + '\\([^]*?^\\}', 'm'))?.[0];
  assert.ok(result, 'exercise actual ' + name);
  return result;
};

function requestedFixture({ projected = false, modules = [{ id: 'knowledge' }], hash = 'late-app' } = {}) {
  const counts = { starts: 0, loads: 0, delays: 0 };
  const catalog = { modules, usedProjectedCatalog: projected };
  const wait = runInNewContext(definition('waitForRequestedHashModule') + '\nwaitForRequestedHashModule', {
    currentHashModuleId: () => hash,
    state: {
      db: { collection: () => ({}) },
      sync: { startCollection: () => { counts.starts++; return new Promise(() => {}); } },
    },
    loadModules: async () => { counts.loads++; return { modules: [{ id: hash }] }; },
    delay: async () => { counts.delays++; },
    console: { log() {}, warn() {} },
  });
  return { wait, catalog, counts };
}

for (const modules of [[{ id: 'knowledge' }], []]) {
  test('projected catalog paints while a missing route sync remains pending: ' + modules.length + ' visible apps', { timeout: 1000 }, async () => {
    const f = requestedFixture({ projected: true, modules });
    const result = await f.wait(f.catalog);
    assert.equal(result, f.catalog, 'return exactly the current authorized catalog');
    assert.equal(f.counts.starts, 1, 'catalog convergence still starts');
    assert.equal(f.counts.loads, 0, 'warm startup must not poll or await the absent app');
    assert.equal(f.counts.delays, 0);
    assert.equal(result.modules.some(mod => mod.id === 'late-app'), false, 'a requested URL must not manufacture an authorized app');
  });
}

test('cold seed retains the existing requested-app wait', { timeout: 1000 }, async () => {
  const f = requestedFixture();
  const result = await f.wait(f.catalog);
  assert.equal(f.counts.loads, 1);
  assert.equal(result.modules[0].id, 'late-app');
});

test('an already available route performs no extra sync wait', { timeout: 1000 }, async () => {
  const f = requestedFixture({ projected: true, modules: [{ id: 'late-app' }] });
  assert.equal(await f.wait(f.catalog), f.catalog);
  assert.equal(f.counts.starts, 0);
  assert.equal(f.counts.loads, 0);
});

test('actual module loader carries projection readiness after allowlist and lifecycle filtering', async () => {
  const load = runInNewContext(definition('loadModules') + '\nloadModules', {
    allowsPackagedModuleCatalogSeed: () => true,
    loadModuleCatalog: async (_timeout, options) => {
      options.startup.usedProjectedCatalog = true;
      return { modules: [{ id: 'allowed' }, { id: 'restricted' }], allowed_module_ids: ['allowed'] };
    },
    ensurePackagedModuleList: async modules => modules,
    normalizeModuleList: modules => modules,
    applyModuleAllowlist: (modules, allowlist) => modules.filter(mod => allowlist.includes(mod.id)),
    filterModulesForAppVersionVisibility: modules => modules,
    moduleCatalogFingerprint: () => 'native-projected',
    state: { governance: null },
  });
  const result = await load();
  assert.equal(result.usedProjectedCatalog, true);
  assert.deepEqual(JSON.parse(JSON.stringify(result.modules)), [{ id: 'allowed' }]);
});

test('catalog refresh still opens the requested app after its real projection arrives', async () => {
  const opened = [];
  const state = { modules: [{ id: 'knowledge' }], activeModule: { id: 'knowledge' }, governance: null };
  const refresh = runInNewContext(definition('refreshModules') + '\nrefreshModules', {
    state,
    moduleRevisionQuery: () => '',
    moduleActivationSignature: () => '',
    loadModules: async () => ({ modules: [{ id: 'knowledge' }, { id: 'late-app' }], catalogFingerprint: 'new-native-catalog' }),
    registerCustomModuleIcons: async () => {},
    normalizeModuleLayout: () => ({}), readModuleLayout: () => ({}),
    persistModuleLayout() {}, renderTabs() {}, scheduleModuleScriptPreload() {},
    refreshRemoteShellStateInBackground() {},
    currentHashModuleId: () => 'late-app',
    openModule: async id => { opened.push(id); },
    console: { log() {}, info() {} },
  });
  await refresh();
  assert.deepEqual(opened, ['late-app']);
});
