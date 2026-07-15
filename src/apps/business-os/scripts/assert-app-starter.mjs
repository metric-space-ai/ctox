#!/usr/bin/env node
import assert from 'node:assert/strict';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const starterRoot = join(appRoot, 'app-starter/v2');
const catalog = JSON.parse(readFileSync(join(starterRoot, 'archetypes.json'), 'utf8'));
const expected = [
  'record-workbench',
  'queue-workflow',
  'editor-document',
  'automation',
  'timeline-thread',
];

assert.equal(catalog.schema, 'ctox.business_os.app_archetypes.v1');
assert.equal(catalog.default, 'record-workbench');
assert.deepEqual(Object.keys(catalog.archetypes), expected);

const fixtureRoot = mkdtempSync(join(tmpdir(), 'ctox-business-os-starter-v2-'));
try {
  for (const archetypeId of expected) {
    const moduleId = `starter-${archetypeId}`;
    const collection = `${moduleId.replaceAll('-', '_')}_records`;
    const moduleDir = join(fixtureRoot, 'business-os/installed-modules', moduleId);
    mkdirSync(join(moduleDir, 'core'), { recursive: true });
    mkdirSync(join(moduleDir, 'tests'), { recursive: true });
    mkdirSync(join(moduleDir, 'locales'), { recursive: true });

    for (const path of ['index.html', 'index.css', 'icon.svg']) {
      cpSync(join(starterRoot, path), join(moduleDir, path));
    }
    for (const locale of ['de.json', 'en.json']) {
      cpSync(join(starterRoot, 'locales', locale), join(moduleDir, 'locales', locale));
    }
    cpSync(join(starterRoot, 'core/automation.mjs'), join(moduleDir, 'core/automation.mjs'));
    cpSync(join(starterRoot, 'tests/records.test.mjs'), join(moduleDir, 'tests/records.test.mjs'));

    const render = (relativePath) => readFileSync(join(starterRoot, relativePath), 'utf8')
      .replaceAll('__MODULE_ID__', moduleId)
      .replaceAll('__COLLECTION__', collection);
    writeFileSync(join(moduleDir, 'index.js'), render('index.js.tpl'));
    writeFileSync(join(moduleDir, 'schema.js'), render('schema.js.tpl'));
    writeFileSync(join(moduleDir, 'collections.schema.json'), render('collections.schema.json.tpl'));
    writeFileSync(join(moduleDir, 'core/records.mjs'), render('core/records.mjs.tpl'));
    writeFileSync(join(moduleDir, 'core/archetype.mjs'), `export const ARCHETYPE = ${JSON.stringify({ id: archetypeId, ...catalog.archetypes[archetypeId] }, null, 2)};\n`);
    writeFileSync(join(moduleDir, 'core/request.mjs'), "export const REQUEST_NOTE = 'Canonical starter contract fixture';\n");
    writeFileSync(join(moduleDir, 'module.json'), `${JSON.stringify({
      id: moduleId,
      title: catalog.archetypes[archetypeId].title,
      description: `Canonical ${archetypeId} starter fixture`,
      entry: `installed-modules/${moduleId}/index.html`,
      icon: 'icon.svg',
      install_scope: 'installed',
      version: '0.1.0',
      collections: [collection],
      category: 'Operations',
      archetype: archetypeId,
      launch_kind: 'desktop-app',
      presentation: {
        default_mode: 'window',
        supported_modes: ['window', 'maximized', 'focus'],
        initial_size: { width: 960, height: 680 },
        minimum_size: { width: 640, height: 480 },
        multi_instance: false,
        auto_restore: false,
      },
      layout: { shell: 'windowed', default_width: 960, default_height: 680, min_width: 640, min_height: 480 },
      store: { generator: 'ctox-runtime-app-starter-v2', archetype: archetypeId },
    }, null, 2)}\n`);

    const template = JSON.parse(readFileSync(join(appRoot, 'template-store', archetypeId, 'template.json'), 'utf8'));
    assert.equal(template.id, archetypeId);
    assert.equal(template.starter_archetype, archetypeId);
    assert.equal(template.source_module, undefined);

    const validation = spawnSync(process.execPath, [
      join(appRoot, 'scripts/validate-app-module.mjs'),
      moduleId,
      '--installed',
      '--workspace',
      fixtureRoot,
      '--json',
    ], { encoding: 'utf8' });
    assert.equal(validation.status, 0, validation.stderr || validation.stdout);
    const result = JSON.parse(validation.stdout);
    assert.equal(result.ok, true, JSON.stringify(result.failures));
  }
} finally {
  rmSync(fixtureRoot, { recursive: true, force: true });
}

const indexJsTemplate = readFileSync(join(starterRoot, 'index.js.tpl'), 'utf8');
assert.match(indexJsTemplate, /ctx\.contextActions\?\.register/);
assert.match(indexJsTemplate, /ctx\.commandBus\.dispatch/);
assert.match(indexJsTemplate, /canWriteCollection/);
assert.match(readFileSync(join(starterRoot, 'index.css'), 'utf8'), /@container business-app-window/);

console.log(`Canonical Business OS app starter OK: ${expected.length} archetypes rendered and validated`);
