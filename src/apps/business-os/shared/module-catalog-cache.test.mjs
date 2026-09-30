import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { runInNewContext } from 'node:vm';

const source = await readFile(new URL('../app.js', import.meta.url), 'utf8');
const fetcher = source.match(/function fetchPackagedModuleRegistry\(\)[\s\S]*?\n\}/)?.[0] || '';
const loader = source.match(/async function loadPackagedModuleCatalog\(\)[\s\S]*?\n\}/)?.[0] || '';
const catalogLoader = source.match(/async function loadModuleCatalog\([^]*?\n\}/)?.[0] || '';
const injectedCatalogLoader = source.match(/function injectedModuleCatalogSnapshot\([^]*?\n\}/)?.[0] || '';
const catalogRevision = source.match(/function moduleCatalogProjectionRevisionMs\([^]*?\n\}/)?.[0] || '';
const initialOpen = source.match(/try \{\n\s+const workspaceSession[\s\S]*?flushDeferredCatalogRefresh\(\);\n\s+\}/)?.[0] || '';
const ensurePackaged = source.match(/async function ensurePackagedModuleList\([^]*?\n\}/)?.[0] || '';
const embeddedPackaged = source.match(/function loadEmbeddedPackagedModuleCatalog\([^]*?\n\}/)?.[0] || '';
const embeddedCatalogSource = source.match(/const OFFLINE_FALLBACK_CATALOG = (\{[\s\S]*?\});\n\/\/ END GENERATED/)?.[1];

assert.match(loader, /fetchPackagedModuleRegistry\(\)/);
assert.match(fetcher, /cache: 'no-store'/);
assert.doesNotMatch(
  fetcher,
  /cache: 'force-cache'/,
  'runtime-installed module releases must not be hidden behind the shell build cache',
);
assert.match(
  loader,
  /const explicitlyAllowedIds = resolveModuleAllowlist\(\)/,
  'the tenant allowlist must make selected packaged apps available without a runtime install',
);
assert.match(
  initialOpen,
  /state\.initialModuleOpened = true;[\s\S]*flushDeferredCatalogRefresh\(\);/,
  'a runtime app that arrives after the first route attempt must not leave catalog refreshes deferred forever',
);
assert.doesNotMatch(
  initialOpen,
  /state\.initialModuleOpened = Boolean\(state\.activeModule\?\.id\)/,
  'catalog refresh readiness describes shell construction, not whether the first requested app already existed',
);
assert.match(
  loader,
  /canonicalSystemIds\.has\(id\) \|\| explicitlyAllowedIds\.has\(id\)/,
  'packaged catalog visibility must stay limited to system apps and explicit tenant selections',
);
assert.match(
  catalogLoader,
  /if \(allowsCompleteQaModuleCatalog\(\)\) \{[\s\S]*?loadPackagedModuleCatalog\(\)/,
  'the isolated all-source QA catalog must not merge runtime or customer modules from RxDB',
);
assert.match(
  injectedCatalogLoader,
  /module_catalog_snapshot/,
  'the server-authoritative bootstrap snapshot must be available while WebRTC catches up',
);
assert.match(
  catalogRevision,
  /catalog\.revision[\s\S]*catalog\.updated_at_ms[\s\S]*catalog\.lastWriteTime/,
  'catalog selection must compare native projection revisions instead of trusting browser cache age',
);
assert.match(
  catalogLoader,
  /moduleCatalogProjectionRevisionMs\(injectedCatalog\) >= moduleCatalogProjectionRevisionMs\(cachedCatalog\)/,
  'a newer native bootstrap projection must replace stale browser catalog metadata',
);
assert.ok(embeddedCatalogSource, 'the generated local startup catalog must exist');
const embeddedCatalog = JSON.parse(embeddedCatalogSource);
const systemManifest = JSON.parse(await readFile(new URL('../system-apps.json', import.meta.url), 'utf8'));
assert.deepEqual(
  embeddedCatalog.modules.filter((mod) => mod.core === true || mod.install_scope === 'core').map((mod) => mod.id).sort(),
  [...systemManifest.apps].sort(),
  'local startup may use only system modules listed in the current shell manifest',
);

{
  let networkCatalogReads = 0;
  let syncStarts = 0;
  const projected = { modules: [{ id: 'desktop', core: true }], updated_at_ms: 10 };
  const startup = {};
  const loadWarmCatalog = runInNewContext(`${catalogLoader}\nloadModuleCatalog`, {
    state: {
      db: { collection: () => ({}) },
      sync: { startCollection: () => { syncStarts += 1; return Promise.resolve(); } },
      shellCatalogMergedIds: new Set(),
    },
    loadQaInstalledModuleCandidate: async () => null,
    normalizeModuleCatalog: (catalog) => catalog,
    withQaInstalledModuleCandidate: (catalog) => catalog,
    allowsCompleteQaModuleCatalog: () => false,
    readModuleCatalogProjection: async () => projected,
    injectedModuleCatalogSnapshot: () => null,
    moduleCatalogProjectionRevisionMs: () => 0,
    loadEmbeddedPackagedModuleCatalog: () => ({ modules: projected.modules }),
    loadPackagedModuleCatalog: async () => { networkCatalogReads += 1; throw new Error('network must not block warm startup'); },
    mergePackagedCatalogModules: (modules) => ({ modules, changedIds: [], changed: false }),
    console,
  });
  const result = await loadWarmCatalog(60000, { allowShellSeed: true, startup });
  assert.deepEqual(JSON.parse(JSON.stringify(result.modules)), projected.modules);
  assert.equal(startup.usedProjectedCatalog, true);
  assert.equal(networkCatalogReads, 0, 'cached projection must not await a packaged network catalog');
  assert.equal(syncStarts, 1, 'catalog delta sync must still start in the background');

  const ensureFromLocal = runInNewContext(`${ensurePackaged}\nensurePackagedModuleList`, {
    loadEmbeddedPackagedModuleCatalog: () => ({ modules: projected.modules }),
    loadPackagedModuleCatalog: async () => { networkCatalogReads += 1; throw new Error('network must not block warm startup'); },
    normalizeModuleList: (modules) => modules,
    moduleBelongsInInstalledCatalog: () => true,
    mergePackagedCatalogModules: (modules) => ({ modules }),
  });
  assert.deepEqual(
    JSON.parse(JSON.stringify(await ensureFromLocal(projected.modules, { allowShellSeed: true, useEmbeddedMetadata: true }))),
    projected.modules,
  );
  assert.equal(networkCatalogReads, 0, 'warm module enrichment must stay local');
}

{
  const embedded = runInNewContext(`${embeddedPackaged}\nloadEmbeddedPackagedModuleCatalog`, {
    getOfflineFallbackCatalog: () => ({ modules: [
      { id: 'desktop', core: true },
      { id: 'allowed-app' },
      { id: 'other-tenant-app' },
    ] }),
    resolveModuleAllowlist: () => new Set(['allowed-app']),
    isSystemModule: (mod) => mod.core === true,
  });
  assert.deepEqual(
    JSON.parse(JSON.stringify(embedded().modules.map((mod) => mod.id))),
    ['desktop', 'allowed-app'],
    'embedded startup metadata must respect the current tenant allowlist',
  );
}

console.log('runtime module catalog cache contract OK');
