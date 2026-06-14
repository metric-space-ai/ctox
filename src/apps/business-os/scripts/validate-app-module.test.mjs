#!/usr/bin/env node
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const validator = fileURLToPath(new URL('./validate-app-module.mjs', import.meta.url));

function runValidator(workspace, moduleId, ...args) {
  return spawnSync(process.execPath, [validator, moduleId, ...args, '--workspace', workspace], {
    encoding: 'utf8',
    maxBuffer: 16 * 1024 * 1024,
  });
}

function writeJson(path, value) {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

function makeWorkspace() {
  const root = mkdtempSync(join(tmpdir(), 'ctox-app-validator-'));
  mkdirSync(join(root, 'src/apps/business-os/modules'), { recursive: true });
  mkdirSync(join(root, 'runtime/business-os/installed-modules'), { recursive: true });
  writeJson(join(root, 'src/apps/business-os/modules/registry.json'), { modules: [] });
  return root;
}

function writeInstalledModule(root, moduleId, overrides = {}) {
  const dir = join(root, 'runtime/business-os/installed-modules', moduleId);
  mkdirSync(join(dir, 'locales'), { recursive: true });
  mkdirSync(join(dir, 'tests'), { recursive: true });
  writeJson(join(dir, 'module.json'), {
    id: moduleId,
    title: moduleId,
    entry: `installed-modules/${moduleId}/index.html`,
    install_scope: 'installed',
    collections: ['business_commands', `${moduleId}_records`],
    layout: { shell: 'full-workspace', left: 'List', center: 'Details' },
    ...overrides.manifest,
  });
  writeJson(join(dir, 'collections.schema.json'), {
    schema_format: 'ctox-business-os-module-collections-v1',
    collections: {
      [`${moduleId}_records`]: {
        version: 0,
        primaryKey: 'id',
        type: 'object',
        properties: {
          id: { type: 'string', maxLength: 120 },
          title: { type: 'string' },
          updated_at_ms: { type: 'number' },
        },
        required: ['id', 'title', 'updated_at_ms'],
      },
      ...overrides.collections,
    },
  });
  writeFileSync(join(dir, 'schema.js'), overrides.schemaJs || [
    'export const collections = {',
    `  ${moduleId}_records: {`,
    '    version: 0,',
    "    primaryKey: 'id',",
    "    type: 'object',",
    "    properties: { id: { type: 'string', maxLength: 120 }, title: { type: 'string' }, updated_at_ms: { type: 'number' } },",
    "    required: ['id', 'title', 'updated_at_ms'],",
    '  },',
    '};',
    '',
  ].join('\n'));
  writeFileSync(join(dir, 'index.html'), overrides.indexHtml || '<main class="good-module"><section>Ready</section></main>\n');
  writeFileSync(join(dir, 'index.css'), overrides.indexCss || '.good-module { --good-accent: #2563eb; color: inherit; }\n');
  writeFileSync(join(dir, 'index.js'), overrides.indexJs || [
    'export async function mount(ctx) {',
    "  ctx.host.textContent = 'Ready';",
    '  return () => { ctx.host.textContent = ""; };',
    '}',
    '',
  ].join('\n'));
  writeFileSync(join(dir, 'icon.svg'), '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"></svg>\n');
  writeJson(join(dir, 'locales/de.json'), { title: moduleId });
  writeJson(join(dir, 'locales/en.json'), { title: moduleId });
  writeFileSync(join(dir, 'tests/basic.test.mjs'), overrides.testJs || [
    "import assert from 'node:assert/strict';",
    'assert.equal(1 + 1, 2);',
    '',
  ].join('\n'));
  return dir;
}

function writeSourceModule(root, moduleId, overrides = {}) {
  const dir = join(root, 'src/apps/business-os/modules', moduleId);
  const docsDir = join(root, 'docs');
  mkdirSync(join(dir, 'locales'), { recursive: true });
  mkdirSync(join(dir, 'tests'), { recursive: true });
  mkdirSync(docsDir, { recursive: true });
  const collectionName = `${moduleId}_records`;
  const manifest = {
    id: moduleId,
    title: moduleId,
    entry: `modules/${moduleId}/index.html`,
    install_scope: 'store',
    collections: ['business_commands', collectionName],
    layout: { shell: 'full-workspace', left: 'List', center: 'Details' },
    ...overrides.manifest,
  };
  writeJson(join(dir, 'module.json'), manifest);
  writeJson(join(dir, 'collections.schema.json'), {
    schema_format: 'ctox-business-os-module-collections-v1',
    collections: {
      [collectionName]: {
        version: 0,
        primaryKey: 'id',
        type: 'object',
        properties: {
          id: { type: 'string', maxLength: 120 },
          title: { type: 'string' },
          updated_at_ms: { type: 'number' },
        },
        required: ['id', 'title', 'updated_at_ms'],
      },
      ...overrides.collections,
    },
  });
  writeFileSync(join(dir, 'schema.js'), overrides.schemaJs || [
    'export const collections = {',
    `  ${collectionName}: {`,
    '    version: 0,',
    "    primaryKey: 'id',",
    "    type: 'object',",
    "    properties: { id: { type: 'string', maxLength: 120 }, title: { type: 'string' }, updated_at_ms: { type: 'number' } },",
    "    required: ['id', 'title', 'updated_at_ms'],",
    '  },',
    '};',
    '',
  ].join('\n'));
  writeFileSync(join(dir, 'index.html'), overrides.indexHtml || '<main class="source-module"><section>Ready</section></main>\n');
  writeFileSync(join(dir, 'index.css'), overrides.indexCss || '.source-module { --source-accent: #2563eb; color: inherit; }\n');
  writeFileSync(join(dir, 'index.js'), overrides.indexJs || [
    'export async function mount(ctx) {',
    "  ctx.host.textContent = 'Ready';",
    '  return () => { ctx.host.textContent = ""; };',
    '}',
    '',
  ].join('\n'));
  writeFileSync(join(dir, 'icon.svg'), '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"></svg>\n');
  writeJson(join(dir, 'locales/de.json'), { title: moduleId });
  writeJson(join(dir, 'locales/en.json'), { title: moduleId });
  writeFileSync(join(dir, 'tests/basic.test.mjs'), overrides.testJs || [
    "import assert from 'node:assert/strict';",
    'assert.equal(1 + 1, 2);',
    '',
  ].join('\n'));
  writeFileSync(join(dir, 'README.md'), `# ${moduleId}\n\nTwo-pane Business OS app module.\n`);
  writeFileSync(join(docsDir, `business-os-${moduleId}-implementation-plan.md`), [
    `# ${moduleId} implementation plan`,
    '',
    '- Build a focused two-pane Business OS module.',
    '- Persist only module-owned records through the provided module data contracts.',
    '- Run the module validator before completion.',
    '',
  ].join('\n'));
  if (overrides.registry !== false) {
    writeJson(join(root, 'src/apps/business-os/modules/registry.json'), {
      modules: [
        {
          id: moduleId,
          title: moduleId,
          entry: manifest.entry,
          install_scope: manifest.install_scope,
          collections: manifest.collections,
        },
      ],
    });
  }
  return dir;
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'good');
  const run = runValidator(root, 'good', '--installed');
  assert.equal(run.status, 0, `${run.stderr}\n${run.stdout}`);
  assert.match(run.stdout, /validation OK: good \(installed mode\)/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'rootalias');
  writeFileSync(join(root, 'harness-module.json'), '{}\n');
  writeFileSync(join(root, 'harness-collections.schema.json'), '{}\n');
  writeFileSync(join(root, 'harness-artifact-status.md'), 'blocked\n');
  const run = runValidator(root, 'rootalias', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /root-level app artifact is forbidden: harness-module\.json/);
  assert.match(run.stderr, /root-level app artifact is forbidden: harness-collections\.schema\.json/);
  assert.match(run.stderr, /root-level app artifact is forbidden: harness-artifact-status\.md/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'rootprobe');
  writeFileSync(join(root, '_test_guard.txt'), 'probe\n');
  const run = runValidator(root, 'rootprobe', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /root-level app artifact is forbidden: _test_guard\.txt/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'testfileprobe');
  writeFileSync(join(root, 'test-file.json'), '{"test":1}\n');
  const run = runValidator(root, 'testfileprobe', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /root-level app artifact is forbidden: test-file\.json/);
}

{
  const root = makeWorkspace();
  const dir = writeInstalledModule(root, 'moduleharnessnote');
  writeFileSync(join(dir, 'HARNESS_ARTIFACT_CONFLICT.md'), 'do not accept this\n');
  const run = runValidator(root, 'moduleharnessnote', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /forbidden module artifact .*HARNESS_ARTIFACT_CONFLICT\.md/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'stalechecker', {
    manifest: { entry: 'modules/stalechecker/index.html' },
  });
  const staleCheckerDir = join(
    root,
    'src/skills/system/product_engineering/business-os-app-module-development/scripts',
  );
  mkdirSync(staleCheckerDir, { recursive: true });
  writeFileSync(join(staleCheckerDir, 'module_static_check.mjs'), 'process.exit(0);\n');
  const run = runValidator(root, 'stalechecker', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /module\.json entry must be installed-modules\/stalechecker\/index\.html/);
}

{
  const root = makeWorkspace();
  writeSourceModule(root, 'sourcegood');
  const run = runValidator(root, 'sourcegood', '--source');
  assert.equal(run.status, 0, `${run.stderr}\n${run.stdout}`);
  assert.match(run.stdout, /validation OK: sourcegood \(source mode\)/);
}

{
  const root = makeWorkspace();
  writeSourceModule(root, 'sourcemissingregistry', { registry: false });
  const run = runValidator(root, 'sourcemissingregistry', '--source');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /registry\.json missing module sourcemissingregistry/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'badmanifest', {
    manifest: {
      entry: 'modules/badmanifest/index.html',
      install_scope: 'store',
      layout: {
        shell: 'full-workspace',
        left: 'List',
        center: 'Details',
        right: 'Inspector',
      },
    },
    schemaJs: [
      'export const collections = {',
      '  business_commands: { version: 0, primaryKey: "id", type: "object", properties: { id: { type: "string" } } },',
      '  badmanifest_records: { version: 0, primaryKey: "id", type: "object", properties: { id: { type: "string" }, title: { type: "string" }, updated_at_ms: { type: "number" } }, required: ["id", "title", "updated_at_ms"] },',
      '};',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'badmanifest', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /module\.json entry must be installed-modules\/badmanifest\/index\.html/);
  assert.match(run.stderr, /module\.json install_scope must be installed/);
  assert.match(run.stderr, /layout\.right requires layout\.third_pane_justification/);
  assert.match(run.stderr, /schema\.js exports shell-registered collection key business_commands/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'webstorage', {
    indexJs: [
      'export async function mount(ctx) {',
      "  localStorage.setItem('webstorage.lastView', 'list');",
      "  ctx.host.textContent = 'Ready';",
      '  return () => { ctx.host.textContent = ""; };',
      '}',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'webstorage', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /localStorage\/sessionStorage persistence/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'syntaxbad', {
    indexJs: 'export async function mount(ctx) {\n  ctx.host.textContent = "broken";\n',
  });
  const run = runValidator(root, 'syntaxbad', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /node --check failed for runtime\/business-os\/installed-modules\/syntaxbad\/index\.js/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'testbad', {
    testJs: [
      "import assert from 'node:assert/strict';",
      "assert.equal('actual', 'expected');",
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'testbad', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /module test failed: runtime\/business-os\/installed-modules\/testbad\/tests\/basic\.test\.mjs/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'dataurltest', {
    testJs: [
      "import { readFile } from 'node:fs/promises';",
      "import { Buffer } from 'node:buffer';",
      "const source = await readFile(new URL('../index.js', import.meta.url), 'utf8');",
      "await import('data:text/javascript;base64,' + Buffer.from(source).toString('base64'));",
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'dataurltest', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /imports local app source through a data: URL/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'directentrytest', {
    testJs: [
      "import { mount } from '../index.js';",
      "if (typeof mount !== 'function') throw new Error('missing mount');",
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'directentrytest', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /imports browser \.js entrypoints directly/);
}

console.log('[validate-app-module.test] OK');
