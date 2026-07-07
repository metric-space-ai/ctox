import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const businessOsRoot = resolve(here, '../..');
const registryPath = resolve(businessOsRoot, 'modules/registry.json');
const appPath = resolve(businessOsRoot, 'app.js');
const appStorePath = resolve(businessOsRoot, 'modules/app-store/index.js');
const desktopPath = resolve(businessOsRoot, 'modules/desktop/index.js');

const registry = JSON.parse(readFileSync(registryPath, 'utf8'));
const appSource = readFileSync(appPath, 'utf8');
const appStoreSource = readFileSync(appStorePath, 'utf8');
const desktopSource = readFileSync(desktopPath, 'utf8');

const modules = Array.isArray(registry.modules) ? registry.modules : [];
const moduleIds = modules.map((mod) => mod.id).filter(Boolean);
assert.equal(new Set(moduleIds).size, moduleIds.length, 'registry module ids must be unique');

const launchableModuleIds = moduleIds.filter((id) => {
  const mod = modules.find((candidate) => candidate.id === id);
  return id !== 'desktop' && id !== 'notizen' && mod?.install_scope !== 'internal';
});

const desktopAppIds = [...appSource.matchAll(/id:\s*'([^']+)'/g)]
  .map((match) => match[1])
  .filter((id) => ['explorer', 'code-editor', 'file-viewer', 'creator'].includes(id))
  .filter((id) => id !== 'file-viewer' && !moduleIds.includes(id));

const launchIds = [...launchableModuleIds, ...desktopAppIds];
assert.equal(new Set(launchIds).size, launchIds.length, 'launch target ids must be unique');

for (const requiredId of ['explorer', 'conversations', 'outbound', 'creator', 'app-store']) {
  assert.ok(launchIds.includes(requiredId), `launch targets must include ${requiredId}`);
}

assert.equal(
  launchIds.filter((id) => id === 'creator').length,
  1,
  'App Creator must have exactly one launch target'
);

for (const requiredId of ['explorer', 'conversations', 'outbound']) {
  assert.ok(
    appSource.includes(requiredId),
    `Start menu source must explicitly include or discover ${requiredId}`
  );
}
assert.ok(appSource.includes('uncategorized'), 'Start menu must render uncategorized launch targets');
assert.ok(appSource.includes('!moduleIds.has(app.id)'), 'Desktop app launcher must skip module id collisions');
assert.ok(
  appSource.includes('moduleBypassesInstanceAllowlist(mod)'),
  'Tenant allowlist must not hide native-visible runtime-installed apps before lifecycle policy filtering'
);
assert.ok(
  appSource.includes('isRuntimeInstalledModule(mod)'),
  'Runtime app allowlist bypass must use the shared Business OS app lifecycle helper'
);
assert.ok(
  appSource.includes("const WINDOW_GEOMETRY_KEY = 'ctox.businessOs.windowGeometry';"),
  'Shell window geometry must have a scoped local persistence cache'
);
assert.ok(
  appSource.includes('persistWindowGeometryLocalCache();'),
  'Shell window geometry saves must update the local cache synchronously'
);
assert.ok(
  appSource.includes('windowGeometryWriteChains'),
  'Shell window geometry RxDB writes must be serialized per window owner'
);
assert.ok(
  appSource.includes('workspace_scope: scope.workspace_scope'),
  'Shell window geometry RxDB records must be scoped to the workspace'
);
assert.ok(
  appSource.includes('actor_scope: scope.actor_scope'),
  'Shell window geometry RxDB records must be scoped to the actor'
);
assert.ok(
  desktopSource.includes('Array.isArray(ctx.modules) ? ctx.modules : await loadModuleRegistry()'),
  'Desktop launcher must use shell-filtered ctx.modules before falling back to registry.json'
);
assert.ok(
  desktopSource.includes("ctx.eventBus.on('modules:changed'"),
  'Desktop launcher must refresh after the RxDB module catalog replaces the startup module snapshot'
);
assert.ok(
  desktopSource.includes('ensureIcons(iconsCollection, launcher)'),
  'Desktop launcher must seed missing icons after module catalog changes'
);
assert.ok(
  desktopSource.includes("const ICON_POSITION_CACHE_KEY = 'ctox.businessOs.desktopIconPositions';"),
  'Desktop icons must keep a scoped local position cache'
);
assert.ok(
  desktopSource.includes('rememberIconPosition(iconId, position, updatedAt);'),
  'Desktop icon drag must write the local position cache before async RxDB persistence'
);
assert.ok(
  desktopSource.includes('docs.forEach((doc, index) => {'),
  'Desktop icon auto-arrange must update the per-user position cache'
);
assert.ok(
  desktopSource.includes('if (!usingFallbackDocs)'),
  'Desktop fallback icons must not overwrite cached persisted positions during transient IndexedDB restarts'
);
assert.ok(
  desktopSource.includes('!launcher.knows(doc.target_module)'),
  'Desktop icons must not render persisted targets outside the current launcher scope'
);
assert.ok(
  desktopSource.includes('icon read skipped during database restart'),
  'Desktop initial icon rendering must tolerate transient IndexedDB connection shutdown'
);
assert.ok(
  desktopSource.includes('layout read skipped during database restart'),
  'Desktop initial layout loading must tolerate transient IndexedDB connection shutdown'
);
assert.ok(
  desktopSource.includes('icon seed skipped during database restart'),
  'Desktop initial icon seeding must tolerate transient IndexedDB connection shutdown'
);
assert.ok(
  desktopSource.includes('return /IDBDatabase.*closing|database connection is closing/i.test(message);'),
  'Desktop transient IndexedDB shutdown detection must not depend on DOMException prototype shape'
);

for (const requiredSnippet of [
  'isLaunchableModule',
  'normalizeDesktopAppItem',
  'uniqueCatalogItems',
  'openDesktopApp',
]) {
  assert.ok(appStoreSource.includes(requiredSnippet), `App Store registry adapter must include ${requiredSnippet}`);
}

console.log(`registry-launch smoke ok: ${launchIds.length} unique launch targets`);
