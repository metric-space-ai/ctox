#!/usr/bin/env node
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const validator = fileURLToPath(new URL('./validate-app-module.mjs', import.meta.url));
const scaffold = fileURLToPath(new URL('./scaffold-app-module.mjs', import.meta.url));

function runValidator(workspace, moduleId, ...args) {
  return spawnSync(process.execPath, [validator, moduleId, ...args, '--workspace', workspace], {
    encoding: 'utf8',
    maxBuffer: 16 * 1024 * 1024,
  });
}

function runScaffold(workspace, moduleId, ...args) {
  return spawnSync(process.execPath, [scaffold, moduleId, ...args, '--workspace', workspace], {
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
  mkdirSync(join(dir, 'core'), { recursive: true });
  writeJson(join(dir, 'module.json'), {
    id: moduleId,
    title: moduleId,
    version: '0.1.0',
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
  writeFileSync(join(dir, 'core/automation.mjs'), overrides.automationJs || [
    'export function buildFollowUpCommand(record = {}) {',
    '  return {',
    "    id: `cmd_${record.id || 'demo'}`,",
    "    module: 'good-module',",
    "    type: 'business_os.chat.task',",
    "    command_type: 'business_os.chat.task',",
    "    record_id: record.id || 'demo',",
    '    payload: {',
    "      title: `Review ${record.title || 'record'}`,",
    "      instruction: `Review ${record.title || 'record'} and create the next CTOX follow-up.`,",
    "      prompt: `Review ${record.title || 'record'} and create the next CTOX follow-up.`,",
    '      record_snapshot: record,',
    "      outbound_channel: 'business_os_chat',",
    "      response_channel: 'business_os_chat',",
    '    },',
    "    client_context: { source: 'validator-fixture', surface: 'validator-fixture.follow-up' },",
    '  };',
    '}',
    '',
  ].join('\n'));
  writeFileSync(join(dir, 'index.html'), overrides.indexHtml || '<main class="good-module"><button type="button" data-action="follow-up">Review</button></main>\n');
  writeFileSync(join(dir, 'index.css'), overrides.indexCss || '.good-module { --good-accent: #2563eb; color: inherit; }\n');
  writeFileSync(join(dir, 'index.js'), overrides.indexJs || [
    "import { buildFollowUpCommand } from './core/automation.mjs';",
    '',
    'async function ensureStyles() {',
    "  if (document.querySelector('link[data-module-styles=\"good\"]')) return;",
    "  const link = document.createElement('link');",
    "  link.rel = 'stylesheet';",
    "  link.href = new URL('./index.css', import.meta.url).href;",
    "  link.dataset.moduleStyles = 'good';",
    "  document.head.append(link);",
    '}',
    '',
    'export async function mount(ctx) {',
    '  await ensureStyles();',
    "  ctx.host.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());",
    "  ctx.host.querySelector('[data-action=\"follow-up\"]')?.addEventListener('click', () => {",
    "    ctx.commandBus.dispatch(buildFollowUpCommand({ id: 'demo', title: 'Demo' }));",
    '  });',
    '  return () => { ctx.host.textContent = ""; };',
    '}',
    '',
  ].join('\n'));
  writeFileSync(join(dir, 'icon.svg'), '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"></svg>\n');
  writeJson(join(dir, 'locales/de.json'), { title: moduleId });
  writeJson(join(dir, 'locales/en.json'), { title: moduleId });
  writeFileSync(join(dir, 'tests/basic.test.mjs'), overrides.testJs || [
    "import assert from 'node:assert/strict';",
    "import { buildFollowUpCommand } from '../core/automation.mjs';",
    "const command = buildFollowUpCommand({ id: 'demo', title: 'Demo' });",
    "assert.equal(command.type, 'business_os.chat.task');",
    "assert.equal(command.command_type, 'business_os.chat.task');",
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

function installedIndexJsWith(extraLines = []) {
  return [
    "import { buildFollowUpCommand } from './core/automation.mjs';",
    '',
    'async function ensureStyles() {',
    "  if (document.querySelector('link[data-module-styles=\"guard\"]')) return;",
    "  const link = document.createElement('link');",
    "  link.rel = 'stylesheet';",
    "  link.href = new URL('./index.css', import.meta.url).href;",
    "  link.dataset.moduleStyles = 'guard';",
    "  document.head.append(link);",
    '}',
    '',
    'export async function mount(ctx) {',
    '  await ensureStyles();',
    "  ctx.host.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());",
    ...extraLines,
    "  ctx.host.querySelector('[data-action=\"follow-up\"]')?.addEventListener('click', () => {",
    "    ctx.commandBus.dispatch(buildFollowUpCommand({ id: 'demo', title: 'Demo' }));",
    '  });',
    '  return () => { ctx.host.textContent = ""; };',
    '}',
    '',
  ].join('\n');
}

{
  const root = makeWorkspace();
  const scaffoldRun = runScaffold(root, 'scaffolded', '--installed', '--title', 'Scaffolded App');
  assert.equal(scaffoldRun.status, 0, `${scaffoldRun.stderr}\n${scaffoldRun.stdout}`);
  const validateRun = runValidator(root, 'scaffolded', '--installed');
  assert.equal(validateRun.status, 0, `${validateRun.stderr}\n${validateRun.stdout}`);
  assert.match(validateRun.stdout, /validation OK: scaffolded \(installed mode\)/);

  const overwriteRun = runScaffold(root, 'scaffolded', '--installed');
  assert.notEqual(overwriteRun.status, 0);
  assert.match(overwriteRun.stderr, /already contains app files/);
}

{
  const root = makeWorkspace();
  const scaffoldRun = runScaffold(root, 'repairmissing', '--installed', '--title', 'Repair Missing');
  assert.equal(scaffoldRun.status, 0, `${scaffoldRun.stderr}\n${scaffoldRun.stdout}`);
  const dir = join(root, 'runtime/business-os/installed-modules/repairmissing');
  rmSync(join(dir, 'core'), { recursive: true, force: true });
  rmSync(join(dir, 'locales'), { recursive: true, force: true });
  rmSync(join(dir, 'tests'), { recursive: true, force: true });
  const repairRun = runScaffold(root, 'repairmissing', '--installed', '--repair-missing', '--json');
  assert.equal(repairRun.status, 0, `${repairRun.stderr}\n${repairRun.stdout}`);
  assert.match(repairRun.stdout, /"repaired": true/);
  const validateRun = runValidator(root, 'repairmissing', '--installed');
  assert.equal(validateRun.status, 0, `${validateRun.stderr}\n${validateRun.stdout}`);
}

{
  const root = makeWorkspace();
  const scaffoldRun = runScaffold(root, 'sourcescaffold', '--source', '--title', 'Source Scaffold');
  assert.equal(scaffoldRun.status, 0, `${scaffoldRun.stderr}\n${scaffoldRun.stdout}`);
  const validateRun = runValidator(root, 'sourcescaffold', '--source');
  assert.equal(validateRun.status, 0, `${validateRun.stderr}\n${validateRun.stdout}`);
  assert.match(validateRun.stdout, /validation OK: sourcescaffold \(source mode\)/);
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
  writeInstalledModule(root, 'constantcommand', {
    automationJs: [
      "const CHAT_COMMAND_TYPE = 'business_os.chat.task';",
      'export function buildFollowUpCommand(record = {}) {',
      '  return {',
      "    id: `cmd_${record.id || 'demo'}`,",
      "    module: 'constantcommand',",
      '    type: CHAT_COMMAND_TYPE,',
      '    command_type: CHAT_COMMAND_TYPE,',
      "    record_id: record.id || 'demo',",
      '    payload: { record_snapshot: record },',
      "    client_context: { source: 'constantcommand', surface: 'constantcommand.follow-up' },",
      '  };',
      '}',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'constantcommand', '--installed');
  assert.equal(run.status, 0, `${run.stderr}\n${run.stdout}`);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'missingimport', {
    indexJs: installedIndexJsWith().replace(
      "import { buildFollowUpCommand } from './core/automation.mjs';",
      [
        "import { missingThing } from './core/missing.mjs';",
        "import { buildFollowUpCommand } from './core/automation.mjs';",
        'void missingThing;',
      ].join('\n'),
    ),
  });
  const run = runValidator(root, 'missingimport', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /relative import \.\/core\/missing\.mjs does not exist/);
}

{
  const root = makeWorkspace();
  const dir = writeInstalledModule(root, 'namedimportmismatch', {
    indexJs: installedIndexJsWith().replace(
      "import { buildFollowUpCommand } from './core/automation.mjs';",
      [
        "import { createRecord } from './core/records.mjs';",
        "import { buildFollowUpCommand } from './core/automation.mjs';",
        'void createRecord;',
      ].join('\n'),
    ),
  });
  writeFileSync(join(dir, 'core/records.mjs'), 'export function normalizeRecord(record = {}) { return record; }\n');
  const run = runValidator(root, 'namedimportmismatch', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /does not provide an export named `createRecord`/);
  assert.match(run.stderr, /Preserve scaffold exports or update every importer/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'selectordrift', {
    indexHtml: '<main class="good-module"><button type="button" data-action="follow-up">Review</button><div data-list></div></main>\n',
    indexJs: installedIndexJsWith([
      "  ctx.host.querySelector('[data-form]')?.addEventListener('submit', () => {});",
    ]),
  });
  const run = runValidator(root, 'selectordrift', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /index\.js queries \[data-form\]/);
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
  const dir = writeInstalledModule(root, 'moduledependencies');
  writeFileSync(join(dir, 'package.json'), '{"type":"module"}\n');
  mkdirSync(join(dir, 'node_modules'), { recursive: true });
  const run = runValidator(root, 'moduledependencies', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /forbidden module artifact .*package\.json/);
  assert.match(run.stderr, /forbidden module artifact .*node_modules/);
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
  writeInstalledModule(root, 'embeddedicon', {
    manifest: {
      layout: {
        shell: 'full-workspace',
        left: 'List',
        center: 'Details',
        icon_svg: '<svg></svg>',
      },
    },
  });
  const run = runValidator(root, 'embeddedicon', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /module\.json layout\.icon_svg is forbidden/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'missingversion', {
    manifest: { version: undefined },
  });
  const run = runValidator(root, 'missingversion', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /module\.json version must be SemVer x\.y\.z without a v prefix/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'legacyversion', {
    manifest: { version: 'v1' },
  });
  const run = runValidator(root, 'legacyversion', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /module\.json version must be SemVer x\.y\.z without a v prefix/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'zeroversion', {
    manifest: { version: '0.0.0' },
  });
  const run = runValidator(root, 'zeroversion', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /module\.json version 0\.0\.0 is not a valid Business OS app work version/);
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
  writeInstalledModule(root, 'networkfetch', {
    indexJs: installedIndexJsWith([
      "  await fetch('/external-service/status');",
    ]),
  });
  const run = runValidator(root, 'networkfetch', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /forbidden installed-app network fetch/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'dynamicimport', {
    indexJs: installedIndexJsWith([
      "  await import('./extra.js');",
    ]),
  });
  const run = runValidator(root, 'dynamicimport', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /forbidden installed-app runtime capability: dynamic import/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'shellglobal', {
    indexJs: installedIndexJsWith([
      "  window.CTOX_BUSINESS_OS_APP.openModule('ctox');",
    ]),
  });
  const run = runValidator(root, 'shellglobal', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /forbidden installed-app runtime capability: Business OS shell global state access/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'cachedfacade', {
    indexJs: installedIndexJsWith([
      '  const db = ctx.db;',
      "  db.collection('cachedfacade_records').find().exec();",
    ]),
  });
  const run = runValidator(root, 'cachedfacade', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /forbidden installed-app runtime capability: cached ctx\.db facade handle/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'controlcommand', {
    indexJs: installedIndexJsWith([
      "  ctx.commandBus.dispatch({ type: 'ctox.module.release', command_type: 'ctox.module.release', payload: {} });",
    ]),
  });
  const run = runValidator(root, 'controlcommand', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /forbidden installed-app runtime capability: direct CTOX control command/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'workerlaunch', {
    indexJs: installedIndexJsWith([
      "  const worker = new Worker(new URL('./worker.js', import.meta.url));",
      '  worker.terminate();',
    ]),
  });
  const run = runValidator(root, 'workerlaunch', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /forbidden installed-app runtime capability: Worker runtime/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'selfrefcss', {
    indexCss: '.good-module { --good-bg: var(--good-bg); color: var(--good-bg); }\n',
  });
  const run = runValidator(root, 'selfrefcss', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /self-referential CSS custom property --good-bg/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'legacychatevent', {
    indexJs: [
      'export async function mount(ctx) {',
      "  window.dispatchEvent(new CustomEvent('ctox-business-os-chat-submit', { detail: { prompt: 'legacy' } }));",
      "  ctx.host.textContent = 'Ready';",
      '  return () => { ctx.host.textContent = ""; };',
      '}',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'legacychatevent', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /legacy shell event dispatch/);
  assert.match(run.stderr, /forbidden legacy shell chat event literal/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'directcommandwrite', {
    indexJs: [
      'export async function mount(ctx) {',
      "  const commands = ctx.db.collection('business_commands');",
      "  await commands.upsert({ id: 'cmd_direct', status: 'submitted' });",
      "  ctx.host.textContent = 'Ready';",
      '  return () => { ctx.host.textContent = ""; };',
      '}',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'directcommandwrite', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /direct business_commands write fallback/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'inlineiconmanifest', {
    manifest: {
      layout: {
        shell: 'full-workspace',
        left: 'List',
        center: 'Details',
        icon_svg: '<svg viewBox="0 0 24 24"></svg>',
      },
    },
  });
  const run = runValidator(root, 'inlineiconmanifest', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /layout\.icon_svg is forbidden/);
  assert.match(run.stderr, /must not embed inline SVG markup/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'fallbacktestliteral', {
    testJs: [
      "import assert from 'node:assert/strict';",
      "import { buildFollowUpCommand } from '../core/automation.mjs';",
      "const command = buildFollowUpCommand({ id: 'demo', title: 'Demo' });",
      "assert.equal(command.type, 'business_os.chat.task');",
      "assert.ok('with business_commands fallback');",
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'fallbacktestliteral', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /forbidden direct command fallback literal/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'noautomation', {
    automationJs: [
      'export function buildFollowUpCommand(record = {}) {',
      "  return { type: 'noop', record_id: record.id || '' };",
      '}',
      '',
    ].join('\n'),
    indexJs: [
      'export async function mount(ctx) {',
      "  ctx.host.textContent = 'Ready';",
      '  return () => { ctx.host.textContent = ""; };',
      '}',
      '',
    ].join('\n'),
    testJs: [
      "import assert from 'node:assert/strict';",
      'assert.equal(1 + 1, 2);',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'noautomation', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /must dispatch at least one real automation through ctx\.commandBus\.dispatch/);
  assert.match(run.stderr, /must include a business_os\.chat\.task automation command/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'missinghtmlmount', {
    indexJs: [
      "import { buildFollowUpCommand } from './core/automation.mjs';",
      '',
      'export async function mount(ctx) {',
      '  const { host } = ctx;',
      "  host.querySelector('[data-action=\"follow-up\"]')?.addEventListener('click', () => {",
      "    ctx.commandBus.dispatch(buildFollowUpCommand({ id: 'demo', title: 'Demo' }));",
      '  });',
      '  return () => { host.innerHTML = ""; };',
      '}',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'missinghtmlmount', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /index\.js must load \.\/index\.html/);
  assert.match(run.stderr, /must render index\.html into ctx\.host\.innerHTML/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'missingcommandtype', {
    automationJs: [
      'export function buildFollowUpCommand(record = {}) {',
      '  return {',
      "    module: 'missingcommandtype',",
      "    type: 'business_os.chat.task',",
      "    record_id: record.id || 'demo',",
      "    payload: { record_snapshot: record },",
      '  };',
      '}',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'missingcommandtype', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /must preserve command_type: business_os\.chat\.task/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'wrongautomation', {
    automationJs: [
      'export function buildFollowUpCommand(record = {}) {',
      '  return {',
      "    module: 'ctox',",
      "    type: 'ctox.business_os.ticket.followup.create',",
      "    command_type: 'ctox.business_os.ticket.followup.create',",
      "    record_id: record.id || 'demo',",
      "    payload: { record_snapshot: record },",
      '  };',
      '}',
      '',
    ].join('\n'),
    testJs: [
      "import assert from 'node:assert/strict';",
      "assert.equal('automation fixture', 'automation fixture');",
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'wrongautomation', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /alternate App Creator automation command/);
  assert.match(run.stderr, /business_os\.chat\.task automation command/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'missingsnapshot', {
    automationJs: [
      'export function buildFollowUpCommand(record = {}) {',
      '  return {',
      "    module: 'missingsnapshot',",
      "    type: 'business_os.chat.task',",
      "    command_type: 'business_os.chat.task',",
      "    record_id: record.id || 'demo',",
      "    payload: { title: record.title || 'Demo' },",
      '  };',
      '}',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'missingsnapshot', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /automation must include a source record_snapshot/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'frameworkruntime', {
    indexJs: [
      'export async function mount(ctx) {',
      "  const view = React.createElement('div', null, 'Nope');",
      '  ctx.host.textContent = String(view);',
      '  return () => { ctx.host.textContent = ""; };',
      '}',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'frameworkruntime', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /vanilla HTML\/CSS\/browser ESM; found React framework runtime/);
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

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'rightreasontest', {
    testJs: [
      "import assert from 'node:assert/strict';",
      "import { buildFollowUpCommand } from '../core/automation.mjs';",
      "assert.equal('right reason', 'right reason');",
      "assert.equal('right selectors', 'right selectors');",
      "assert.equal(buildFollowUpCommand({ id: 'demo' }).command_type, 'business_os.chat.task');",
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'rightreasontest', '--installed');
  assert.equal(run.status, 0, `${run.stderr}\n${run.stdout}`);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'rightresizertestliteral', {
    testJs: [
      "import assert from 'node:assert/strict';",
      "assert.equal('right-resizer', 'right-resizer');",
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'rightresizertestliteral', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /forbidden third-pane literal right-resizer/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'scannerevasiontest', {
    testJs: [
      "import assert from 'node:assert/strict';",
      'const legacyTokens = [String.fromCharCode(120)];',
      "assert.equal(legacyTokens[0], 'x');",
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'scannerevasiontest', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /validator scanner-evasion/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'sourceabsence', {
    testJs: [
      "import assert from 'node:assert/strict';",
      "const indexSource = 'export async function mount(ctx) {}';",
      "assert.doesNotMatch(indexSource, /ctx\\.db\\.raw/);",
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'sourceabsence', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /source absence assertion/);
}

console.log('[validate-app-module.test] OK');
