import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '../../../..');
const modulesRoot = path.join(repoRoot, 'src/apps/business-os/modules');
const registryPath = path.join(modulesRoot, 'registry.json');
const systemAppsPath = path.join(repoRoot, 'src/apps/business-os/system-apps.json');

export const COMPATIBILITY_DESKTOP_APPS = Object.freeze([
  Object.freeze({ id: 'explorer', title: 'Files', kind: 'desktop-app', cohort: 'compatibility' }),
  Object.freeze({ id: 'code-editor', title: 'Source Editor', kind: 'desktop-app', cohort: 'compatibility' }),
]);

export function loadBusinessOsAppInventory() {
  const registry = JSON.parse(fs.readFileSync(registryPath, 'utf8'));
  const systemAppsManifest = JSON.parse(fs.readFileSync(systemAppsPath, 'utf8'));
  const systemAppIds = Array.isArray(systemAppsManifest?.apps)
    ? systemAppsManifest.apps.map((id) => String(id || '').trim()).filter(Boolean)
    : [];
  const modules = Array.isArray(registry?.modules) ? registry.modules : [];
  const registryIds = modules.map((module) => String(module?.id || '').trim()).filter(Boolean);
  const sourceIds = fs.readdirSync(modulesRoot, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .filter((id) => fs.existsSync(path.join(modulesRoot, id, 'module.json')))
    .sort();

  assertUnique(registryIds, 'modules/registry.json');
  assertUnique(systemAppIds, 'system-apps.json');
  assertSameIds(registryIds, sourceIds);

  const sourceApps = modules.map((module) => Object.freeze({
    id: module.id,
    title: module.title || module.id,
    kind: module.id === 'desktop' ? 'shell-surface' : 'module',
    cohort: module.install_scope === 'core' ? 'core' : 'store',
    installScope: module.install_scope || '',
    aliases: module.id === 'notes' ? ['Notes'] : [],
  }));
  const coreApps = sourceApps.filter((app) => app.installScope === 'core');
  assertExactIds(systemAppIds, coreApps.map((app) => app.id), 'system-apps.json', 'core module manifests');

  if (sourceApps.length !== 34) {
    throw new Error(`Business OS source inventory must contain exactly 34 apps; found ${sourceApps.length}`);
  }
  if (coreApps.length !== 11) {
    throw new Error(`Business OS system inventory must contain exactly 11 apps; found ${coreApps.length}`);
  }

  return Object.freeze({
    registryPath,
    systemAppsPath,
    systemAppIds: Object.freeze(systemAppIds),
    coreApps: Object.freeze(coreApps),
    sourceApps: Object.freeze(sourceApps),
    compatibilityApps: COMPATIBILITY_DESKTOP_APPS,
    allApps: Object.freeze([...sourceApps, ...COMPATIBILITY_DESKTOP_APPS]),
  });
}

function assertExactIds(expectedIds, actualIds, expectedSource, actualSource) {
  const expected = new Set(expectedIds);
  const actual = new Set(actualIds);
  const missing = expectedIds.filter((id) => !actual.has(id));
  const unexpected = actualIds.filter((id) => !expected.has(id));
  if (missing.length || unexpected.length) {
    throw new Error([
      `Business OS system app drift between ${expectedSource} and ${actualSource}.`,
      `missing: ${missing.join(', ') || 'none'}`,
      `unexpected: ${unexpected.join(', ') || 'none'}`,
    ].join(' '));
  }
}

function assertUnique(ids, source) {
  const duplicates = ids.filter((id, index) => ids.indexOf(id) !== index);
  if (duplicates.length) {
    throw new Error(`${source} contains duplicate module ids: ${[...new Set(duplicates)].join(', ')}`);
  }
}

function assertSameIds(registryIds, sourceIds) {
  const registry = new Set(registryIds);
  const source = new Set(sourceIds);
  const missingFromRegistry = sourceIds.filter((id) => !registry.has(id));
  const missingFromSource = registryIds.filter((id) => !source.has(id));
  if (missingFromRegistry.length || missingFromSource.length) {
    throw new Error([
      'Business OS registry/source inventory drift detected.',
      `missing from registry: ${missingFromRegistry.join(', ') || 'none'}`,
      `missing from source: ${missingFromSource.join(', ') || 'none'}`,
    ].join(' '));
  }
}
