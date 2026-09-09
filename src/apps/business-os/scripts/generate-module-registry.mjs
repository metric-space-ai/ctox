#!/usr/bin/env node
// Generates modules/registry.json and the offline-fallback catalog block in
// app.js from the module manifests (modules/<id>/module.json). The manifests
// own module metadata. The frozen operator selection owns approved icon metadata;
// this script projects both without changing the selected artwork.
//
//   node scripts/generate-module-registry.mjs          # rewrite both outputs
//   node scripts/generate-module-registry.mjs --check  # fail on any drift
//
// Validation is strict on purpose: a manifest that disagrees with the core
// membership owned by system-apps.json is a defect to fix in the manifest,
// never something this generator papers over.

import { readFileSync, readdirSync, writeFileSync, existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
const checkOnly = process.argv.includes('--check');

const BEGIN_MARKER = '// BEGIN GENERATED offline-fallback-catalog (node scripts/generate-module-registry.mjs — do not edit by hand)';
const END_MARKER = '// END GENERATED offline-fallback-catalog';

const REGISTRY_ENTRY_KEYS = [
  'id',
  'title',
  'description',
  'entry',
  'collections',
  'layout',
  'category',
  'version',
  'developer',
  'license',
  'tags',
  'store',
  'install_scope',
  'default_installed',
  'launch_kind',
  'presentation',
  'source',
  'core',
  'editable',
  'deletable',
];

function fail(message) {
  console.error(`generate-module-registry: ${message}`);
  process.exit(1);
}

function readJson(path) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

const iconManifest = readJson(resolve(root, 'shared/assets/workjet-icons/operator-selection-v1/manifest.json'));
if (iconManifest.schema !== 'workjet.operator-icon-selection.v1'
  || !Array.isArray(iconManifest.icons)
  || iconManifest.icons.length !== iconManifest.count) fail('invalid operator icon selection');
const selectedIcons = new Map(iconManifest.icons.map((icon) => [icon.appId, icon]));
if (selectedIcons.size !== iconManifest.icons.length) fail('duplicate operator icon selection');

const system = readJson(resolve(root, 'system-apps.json'));
const systemIds = system.apps;
if (!Array.isArray(systemIds) || systemIds.length === 0) fail('system-apps.json has no apps');

const moduleDirs = readdirSync(resolve(root, 'modules'), { withFileTypes: true })
  .filter((d) => d.isDirectory())
  .map((d) => d.name)
  .filter((name) => existsSync(resolve(root, 'modules', name, 'module.json')));

const manifests = new Map();
for (const id of moduleDirs) {
  const manifest = readJson(resolve(root, 'modules', id, 'module.json'));
  if (manifest.id !== id) fail(`modules/${id}/module.json declares id "${manifest.id}"`);
  manifests.set(id, manifest);
}

// --- validation: core membership is owned by system-apps.json ---------------
const coreScopeIds = moduleDirs.filter((id) => manifests.get(id).install_scope === 'core');
const systemSet = new Set(systemIds);
for (const id of systemIds) {
  if (!manifests.has(id)) fail(`system app "${id}" has no modules/${id}/module.json`);
  const m = manifests.get(id);
  const checks = [
    ['install_scope', 'core'],
    ['default_installed', true],
    ['source', 'core'],
    ['core', true],
    ['deletable', false],
  ];
  for (const [field, expected] of checks) {
    if (m[field] !== expected) fail(`system app "${id}" manifest ${field} must be ${JSON.stringify(expected)}, got ${JSON.stringify(m[field])}`);
  }
  if (m.store?.installable !== false) fail(`system app "${id}" manifest store.installable must be false`);
  if (m.store?.editable_after_install !== false) fail(`system app "${id}" manifest store.editable_after_install must be false`);
  if (m.store?.distribution !== 'system-module') fail(`system app "${id}" manifest store.distribution must be "system-module"`);
}
for (const id of coreScopeIds) {
  if (!systemSet.has(id)) fail(`modules/${id}/module.json claims install_scope "core" but "${id}" is not in system-apps.json (core membership is owned there)`);
}
for (const id of moduleDirs) {
  const m = manifests.get(id);
  if (!systemSet.has(id)) {
    if (m.install_scope === 'core' || m.install_scope === 'starter') fail(`non-system module "${id}" must not declare install_scope "${m.install_scope}"`);
    if (m.core === true) fail(`non-system module "${id}" must not declare core:true`);
    if (m.default_installed === true) fail(`non-system module "${id}" must not declare default_installed:true`);
  }
}

// --- projection -------------------------------------------------------------
function registryEntry(manifest) {
  const entry = {};
  for (const key of REGISTRY_ENTRY_KEYS) {
    if (Object.prototype.hasOwnProperty.call(manifest, key)) entry[key] = manifest[key];
  }
  const icon = selectedIcons.get(manifest.id);
  if (icon) {
    entry.layout = {
      ...entry.layout,
      icon_asset: icon.renderAsset,
      icon_asset_sha256: icon.renderSha256,
      icon_selection_sha256: icon.sha256,
      icon_selection_candidate: icon.candidateId,
      icon_asset_kind: iconManifest.assetKind,
    };
  }
  return entry;
}

const orderedIds = [
  'desktop',
  ...moduleDirs.filter((id) => id !== 'desktop').sort((a, b) => a.localeCompare(b, 'en')),
];
const registry = {
  ok: true,
  modules: orderedIds.map((id) => registryEntry(manifests.get(id))),
};
const registryJson = `${JSON.stringify(registry, null, 2)}\n`;

const fallback = {
  ok: true,
  modules: systemIds.map((id) => registryEntry(manifests.get(id))),
};
const fallbackBlock = [
  BEGIN_MARKER,
  `const OFFLINE_FALLBACK_CATALOG = ${JSON.stringify(fallback, null, 2)};`,
  END_MARKER,
].join('\n');

// --- outputs ----------------------------------------------------------------
const registryPath = resolve(root, 'modules', 'registry.json');
const appJsPath = resolve(root, 'app.js');

const appJs = readFileSync(appJsPath, 'utf8');
const beginIdx = appJs.indexOf(BEGIN_MARKER);
const endIdx = appJs.indexOf(END_MARKER);
if (beginIdx === -1 || endIdx === -1 || endIdx < beginIdx) {
  fail('app.js is missing the generated offline-fallback-catalog markers');
}
const nextAppJs = appJs.slice(0, beginIdx) + fallbackBlock + appJs.slice(endIdx + END_MARKER.length);

const currentRegistry = readFileSync(registryPath, 'utf8');
const registryDrift = currentRegistry !== registryJson;
const appJsDrift = nextAppJs !== appJs;

if (checkOnly) {
  if (registryDrift || appJsDrift) {
    if (registryDrift) console.error('drift: modules/registry.json does not match the manifests');
    if (appJsDrift) console.error('drift: app.js offline-fallback-catalog block does not match the manifests');
    console.error('run: node src/apps/business-os/scripts/generate-module-registry.mjs (and bump APP_BUILD when app.js changed)');
    process.exit(1);
  }
  console.log(`module_registry_generated_ok=1 modules=${orderedIds.length} fallback=${systemIds.length}`);
  process.exit(0);
}

if (registryDrift) writeFileSync(registryPath, registryJson);
if (appJsDrift) writeFileSync(appJsPath, nextAppJs);
console.log(`module_registry_written registry_changed=${registryDrift} app_js_changed=${appJsDrift} modules=${orderedIds.length} fallback=${systemIds.length}`);
