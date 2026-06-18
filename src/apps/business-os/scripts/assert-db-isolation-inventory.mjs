// Business OS Phase 13A/13D DB-isolation inventory guard.
//
// This is intentionally an inventory/drift guard, not the Phase 13C migration
// guard. Existing packaged/core modules and privileged shell surfaces may still
// use the compatibility DB facade, but every raw/property/proxy/cache access
// shape must be explicitly classified with an owner and review date before
// migration starts.

import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const appRoot = resolve(scriptDir, '..');
const repoRoot = resolve(appRoot, '../../..');
const inventoryPath = join(repoRoot, 'docs/business-os-db-isolation-inventory.json');
const modulesRoot = join(appRoot, 'modules');
const desktopAppsRoot = join(appRoot, 'desktop-apps');
const appJsPath = join(appRoot, 'app.js');

const offenders = [];

if (process.argv.includes('--self-test')) {
  runSelfTest();
  process.exit(0);
}

const inventory = readJson(inventoryPath);

if (inventory?.schema_version !== 'business-os-db-isolation-inventory-v1') {
  offenders.push('inventory schema_version must be business-os-db-isolation-inventory-v1');
}

validateModules();
validateDesktopApps();
validateUnscopedFacades();
validateScopedSystemModuleExceptions();

if (offenders.length) {
  console.error(`Business OS DB-isolation inventory failed:\n${offenders.map((line) => `- ${line}`).join('\n')}`);
  process.exit(1);
}

console.log(
  `Business OS DB-isolation inventory OK (${Object.keys(inventory.modules || {}).length} modules, `
  + `${Object.keys(inventory.desktop_apps || {}).length} desktop apps, `
  + `${(inventory.unscoped_facades || []).length} unscoped facades)`,
);

function validateModules() {
  const actual = moduleManifests();
  const declared = inventory.modules || {};
  compareIds('modules', Object.keys(actual), Object.keys(declared));

  for (const [id, manifest] of Object.entries(actual)) {
    const entry = declared[id];
    if (!entry) continue;
    const moduleDir = join(modulesRoot, id);
    const access = detectModuleDbAccess(moduleDir);
    expectEqual(`modules.${id}.manifest_path`, entry.manifest_path, relative(repoRoot, manifest.path));
    expectEqual(`modules.${id}.source_path`, entry.source_path, `src/apps/business-os/modules/${id}`);
    expectEqual(`modules.${id}.title`, entry.title, manifest.data.title || id);
    expectEqual(`modules.${id}.install_scope`, entry.install_scope, manifest.data.install_scope || '');
    expectEqual(`modules.${id}.distribution`, entry.distribution, manifest.data.store?.distribution || '');
    expectEqual(`modules.${id}.default_installed`, Boolean(entry.default_installed), Boolean(manifest.data.default_installed));
    validateModuleDbAccess(`modules.${id}.db_access`, entry.db_access, {
      uses_raw_db: access.usesRawDb,
      uses_collection_property_access: access.usesCollectionPropertyAccess,
      uses_collections_proxy: access.usesCollectionsProxy,
      uses_cached_db_handle: access.usesCachedDbHandle,
    });
    validateOwnedClassification(`modules.${id}`, entry);
  }
}

function validateDesktopApps() {
  const actual = desktopApps();
  const declared = inventory.desktop_apps || {};
  compareIds('desktop_apps', actual, Object.keys(declared));
  for (const id of actual) {
    const entry = declared[id];
    if (!entry) continue;
    expectEqual(`desktop_apps.${id}.source_path`, entry.source_path, `src/apps/business-os/desktop-apps/${id}`);
    validateOwnedClassification(`desktop_apps.${id}`, entry);
  }
}

function validateUnscopedFacades() {
  const actual = detectUnscopedFacades();
  const declared = Array.isArray(inventory.unscoped_facades) ? inventory.unscoped_facades : [];
  compareIds('unscoped_facades', actual.map((entry) => entry.id), declared.map((entry) => entry.id));
  for (const actualEntry of actual) {
    const entry = declared.find((candidate) => candidate.id === actualEntry.id);
    if (!entry) continue;
    expectEqual(
      `unscoped_facades.${actualEntry.id}.source_path`,
      stripLineNumber(entry.source_path),
      stripLineNumber(actualEntry.source_path),
    );
    expectEqual(`unscoped_facades.${actualEntry.id}.owner_function`, entry.owner_function, actualEntry.owner_function);
    validateOwnedClassification(`unscoped_facades.${actualEntry.id}`, entry);
  }
}

function validateScopedSystemModuleExceptions() {
  const appScopedCollections = readScopedSystemModuleDbCollections();
  const scopedStatuses = new Set([
    'system-scoped-exception-tested',
    'internal-scoped-exception-tested',
  ]);
  for (const [id, entry] of Object.entries(inventory.modules || {})) {
    const status = String(entry?.isolation_status || '');
    if (status.includes('pending-review')) {
      offenders.push(`modules.${id}.isolation_status must not remain ${status}`);
    }
    if (!scopedStatuses.has(status)) continue;
    const path = `modules.${id}`;
    if (!Array.isArray(entry.scoped_collections) || !entry.scoped_collections.length) {
      offenders.push(`${path}.scoped_collections is required for ${status}`);
      continue;
    }
    const declared = normalizeStringList(entry.scoped_collections);
    const actual = appScopedCollections.get(id) || [];
    expectArrayEqual(`${path}.scoped_collections`, declared, actual);
    validateModuleDbAccess(`${path}.db_access`, entry.db_access, {
      uses_raw_db: false,
      uses_collection_property_access: false,
      uses_collections_proxy: false,
      uses_cached_db_handle: false,
    });
  }
}

function moduleManifests() {
  const result = {};
  for (const id of readdirSync(modulesRoot).sort()) {
    const dir = join(modulesRoot, id);
    const manifestPath = join(dir, 'module.json');
    if (!statSync(dir).isDirectory() || !existsSync(manifestPath)) continue;
    result[id] = {
      path: manifestPath,
      data: readJson(manifestPath),
    };
  }
  return result;
}

function desktopApps() {
  return readdirSync(desktopAppsRoot)
    .filter((id) => existsSync(join(desktopAppsRoot, id, 'app.js')))
    .sort();
}

function detectModuleDbAccess(moduleDir) {
  const source = readSourceFiles(moduleDir)
    .map((file) => readFileSync(file, 'utf8'))
    .join('\n');
  return detectModuleDbAccessFromSource(source);
}

function detectModuleDbAccessFromSource(source) {
  const normalized = String(source || '')
    .replace(/\?\.\(/g, '(')
    .replace(/\?\.(?=[A-Za-z_$\[])/g, '.');
  const dbRootPattern = String.raw`(?:ctx|state\.ctx|STATE\.ctx|stateRef\.ctx)\.db`;
  const aliases = [...normalized.matchAll(new RegExp(String.raw`\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*${dbRootPattern}(?!\s*(?:\.|\[))\b`, 'g'))]
    .map((match) => match[1])
    .filter(Boolean);
  const dbRefs = [dbRootPattern, ...aliases.map(escapeRegExp)].join('|');
  const dbRefPattern = String.raw`(?:${dbRefs})`;
  return {
    usesRawDb: new RegExp(String.raw`\b${dbRefPattern}\s*(?:\.raw\b|\[\s*['"]raw['"]\s*\])`).test(normalized)
      || new RegExp(String.raw`\bsetBusinessOsRawDatabase\??\.\([^)]*${dbRootPattern}\s*(?:\.raw\b|\[\s*['"]raw['"]\s*\])`).test(normalized),
    usesCollectionPropertyAccess: new RegExp(String.raw`\b${dbRefPattern}\s*(?:\.(?!collection\s*\(|raw\b|collections\b)[A-Za-z_$][\w$]*\b|\.\s*\[[^\]]+\])`).test(normalized),
    usesCollectionsProxy: new RegExp(String.raw`\b${dbRefPattern}\s*\.collections\b`).test(normalized),
    usesCachedDbHandle: new RegExp(String.raw`\b__ctx__db\s*=\s*${dbRootPattern}\b|\bglobalThis\.[A-Za-z_$][\w$]*\s*=\s*${dbRootPattern}\b`).test(normalized),
  };
}

function escapeRegExp(value) {
  return String(value || '').replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function readSourceFiles(root) {
  const result = [];
  const stack = [root];
  while (stack.length) {
    const dir = stack.pop();
    for (const name of readdirSync(dir)) {
      const path = join(dir, name);
      const stat = statSync(path);
      if (stat.isDirectory()) {
        if (name !== 'node_modules' && name !== '.git') stack.push(path);
      } else if (/\.(?:js|mjs)$/.test(name)) {
        result.push(path);
      }
    }
  }
  return result.sort();
}

function detectUnscopedFacades() {
  const source = readFileSync(appJsPath, 'utf8');
  const entries = [];
  const matcher = /createLiveDbFacade\s*\(\s*\)/g;
  let match;
  while ((match = matcher.exec(source))) {
    const before = source.slice(Math.max(0, match.index - 900), match.index);
    const line = lineNumber(source, match.index);
    const fallbackOwnerFunction = enclosingFunctionName(source, match.index);
    const id = classifyUnscopedFacade(before, fallbackOwnerFunction, line);
    entries.push({
      id,
      source_path: `src/apps/business-os/app.js:${line}`,
      owner_function: ownerFunctionForUnscopedFacade(id, fallbackOwnerFunction),
    });
  }
  return entries.sort((a, b) => a.id.localeCompare(b.id));
}

function readScopedSystemModuleDbCollections(source = readFileSync(appJsPath, 'utf8')) {
  const blockMatch = source.match(/const\s+SCOPED_SYSTEM_MODULE_DB_COLLECTIONS\s*=\s*Object\.freeze\(\s*\{([\s\S]*?)\n\}\);\s*/);
  if (!blockMatch) {
    offenders.push('SCOPED_SYSTEM_MODULE_DB_COLLECTIONS was not found in src/apps/business-os/app.js');
    return new Map();
  }
  const result = new Map();
  const entryPattern = /(?:^|\n)\s*(?:(['"])([^'"]+)\1|([A-Za-z_$][\w$]*))\s*:\s*Object\.freeze\(\s*\[([\s\S]*?)\]\s*\)\s*,?/g;
  let match;
  while ((match = entryPattern.exec(blockMatch[1]))) {
    const id = match[2] || match[3];
    const collections = [];
    const collectionPattern = /['"]([^'"]+)['"]/g;
    let collectionMatch;
    while ((collectionMatch = collectionPattern.exec(match[4]))) {
      collections.push(collectionMatch[1]);
    }
    result.set(id, normalizeStringList(collections));
  }
  return result;
}

function classifyUnscopedFacade(before, ownerFunction, line) {
  if (/openReactSettings\s*\(\s*\{[\s\S]*$/.test(before)) return 'settings-drawer-react-settings';
  if (/appModule\.mount\s*\(\s*win\.container\s*,\s*\{[\s\S]*$/.test(before)) return 'desktop-app-window-context';
  if (/initBusinessChat\s*\(\s*\{[\s\S]*$/.test(before)) return 'business-chat-companion';
  if (/initBusinessReporter\s*\(\s*\{[\s\S]*$/.test(before)) return 'business-reporter-companion';
  return `${ownerFunction || 'unknown'}:${line}`;
}

function ownerFunctionForUnscopedFacade(id, fallback) {
  if (id === 'settings-drawer-react-settings') return 'openSettingsDrawer';
  if (id === 'desktop-app-window-context') return 'openDesktopApp';
  if (id === 'business-chat-companion' || id === 'business-reporter-companion') {
    return 'scheduleBusinessCompanions';
  }
  return fallback || '';
}

function enclosingFunctionName(source, index) {
  const prefix = source.slice(0, index);
  const matches = [...prefix.matchAll(/(?:^|\n)function\s+([A-Za-z_$][\w$]*)\s*\(/g)];
  return matches.at(-1)?.[1] || '';
}

function lineNumber(source, index) {
  return source.slice(0, index).split('\n').length;
}

function validateOwnedClassification(path, entry) {
  for (const key of ['classification', 'isolation_status', 'owner', 'review_by']) {
    if (!entry?.[key]) offenders.push(`${path}.${key} is required`);
  }
  if (entry?.review_by && !/^\d{4}-\d{2}-\d{2}$/.test(entry.review_by)) {
    offenders.push(`${path}.review_by must be YYYY-MM-DD`);
  }
}

function validateModuleDbAccess(path, entry, expected) {
  if (!entry || typeof entry !== 'object' || Array.isArray(entry)) {
    offenders.push(`${path} must be an object`);
    return;
  }
  for (const [key, expectedValue] of Object.entries(expected)) {
    if (typeof entry[key] !== 'boolean') {
      offenders.push(`${path}.${key} must be a boolean`);
      continue;
    }
    expectEqual(`${path}.${key}`, entry[key], expectedValue);
  }
}

function normalizeStringList(values) {
  return [...new Set((Array.isArray(values) ? values : [])
    .map((value) => String(value || '').trim())
    .filter(Boolean))].sort();
}

function expectArrayEqual(path, actual, expected) {
  const normalizedActual = normalizeStringList(actual);
  const normalizedExpected = normalizeStringList(expected);
  if (JSON.stringify(normalizedActual) !== JSON.stringify(normalizedExpected)) {
    offenders.push(`${path} expected ${JSON.stringify(normalizedExpected)} but found ${JSON.stringify(normalizedActual)}`);
  }
}

function compareIds(label, actualIds, declaredIds) {
  const actual = new Set(actualIds);
  const declared = new Set(declaredIds);
  for (const id of [...actual].sort()) {
    if (!declared.has(id)) offenders.push(`${label}: missing inventory entry for ${id}`);
  }
  for (const id of [...declared].sort()) {
    if (!actual.has(id)) offenders.push(`${label}: stale inventory entry for ${id}`);
  }
}

function expectEqual(path, actual, expected) {
  if (actual !== expected) {
    offenders.push(`${path} expected ${JSON.stringify(expected)} but found ${JSON.stringify(actual)}`);
  }
}

function stripLineNumber(path) {
  return String(path || '').replace(/:\d+$/, '');
}

function readJson(path) {
  try {
    return JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    console.error(`Failed to read JSON ${path}: ${String(error?.message || error)}`);
    process.exit(1);
  }
}

function runSelfTest() {
  const cases = [
    {
      name: 'optional raw db access',
      source: 'const commands = state.ctx.db?.raw?.business_commands;',
      expected: { usesRawDb: true },
    },
    {
      name: 'optional chained raw db access',
      source: 'return state.ctx?.db?.raw?.[name] || null;',
      expected: { usesRawDb: true },
    },
    {
      name: 'raw db forwarded into helper',
      source: 'dataSource.setBusinessOsRawDatabase?.(ctx.db?.raw || null);',
      expected: { usesRawDb: true },
    },
    {
      name: 'collection property access',
      source: 'await state.ctx.db.documents.findOne(id).exec();',
      expected: { usesCollectionPropertyAccess: true },
    },
    {
      name: 'dynamic collection property access',
      source: 'const collection = ctx?.db?.[name] || ctx?.db?.collection?.(name);',
      expected: { usesCollectionPropertyAccess: true },
    },
    {
      name: 'collections proxy access',
      source: 'const col = STATE.ctx.db.collections?.[name];',
      expected: { usesCollectionsProxy: true },
    },
    {
      name: 'local db alias raw access',
      source: 'const db = state.ctx?.db; return db?.raw?.[name] || db?.collection?.(name);',
      expected: { usesRawDb: true },
    },
    {
      name: 'local db alias collections proxy access',
      source: 'let db = ctx?.db; const collection = db?.collections?.[name];',
      expected: { usesCollectionsProxy: true },
    },
    {
      name: 'cached db handle on dom node',
      source: 'els.activeEmployeeList.__ctx__db = ctx.db;',
      expected: { usesCachedDbHandle: true },
    },
    {
      name: 'cached db handle on global',
      source: 'globalThis.CTOX_ACTIVE_DB = ctx.db;',
      expected: { usesCachedDbHandle: true },
    },
    {
      name: 'safe collection factory call',
      source: "const collection = ctx.db?.collection?.('business_commands');",
      expected: {
        usesRawDb: false,
        usesCollectionPropertyAccess: false,
        usesCollectionsProxy: false,
        usesCachedDbHandle: false,
      },
    },
  ];
  const failures = [];
  for (const testCase of cases) {
    const actual = detectModuleDbAccessFromSource(testCase.source);
    for (const [key, expected] of Object.entries(testCase.expected)) {
      if (actual[key] !== expected) {
        failures.push(`${testCase.name}: ${key} expected ${expected} but found ${actual[key]}`);
      }
    }
  }
  const scopedFixture = `
const SCOPED_SYSTEM_MODULE_DB_COLLECTIONS = Object.freeze({
  'app-store': Object.freeze([
    'business_commands',
    'business_module_catalog',
  ]),
  ctox: Object.freeze([
    'ctox_runtime_settings',
  ]),
});
`;
  const scoped = readScopedSystemModuleDbCollections(scopedFixture);
  if (JSON.stringify(scoped.get('app-store')) !== JSON.stringify(['business_commands', 'business_module_catalog'])) {
    failures.push('scoped system parser did not read quoted module ids');
  }
  if (JSON.stringify(scoped.get('ctox')) !== JSON.stringify(['ctox_runtime_settings'])) {
    failures.push('scoped system parser did not read identifier module ids');
  }
  if (failures.length) {
    console.error(`Business OS DB-isolation inventory self-test failed:\n${failures.map((line) => `- ${line}`).join('\n')}`);
    process.exit(1);
  }
  console.log(`Business OS DB-isolation inventory self-test OK (${cases.length} cases)`);
}
