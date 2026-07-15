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
  writeJson(join(root, 'package.json'), { type: 'module' });
  writeJson(join(root, 'src/apps/business-os/modules/registry.json'), { modules: [] });
  return root;
}

function installedIndexJs(moduleId, collectionName, extraLines = []) {
  return [
    "import { buildFollowUpCommand } from './core/automation.mjs';",
    '',
    'function attachStylesheetOnce() {',
    "  if (document.querySelector('link[data-module-styles=\"validator\"]')) return;",
    "  const link = document.createElement('link');",
    "  link.rel = 'stylesheet';",
    "  link.href = new URL('./index.css', import.meta.url).href;",
    "  link.dataset.moduleStyles = 'validator';",
    '  document.head.append(link);',
    '}',
    '',
    'export async function mount(ctx) {',
    '  attachStylesheetOnce();',
    "  ctx.host.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());",
    `  const records = ctx.db.collection('${collectionName}');`,
    '  void records;',
    ...extraLines,
    "  ctx.host.querySelector('[data-action=\"create-record\"]')?.addEventListener('click', () => {",
    "    records?.upsert?.({ id: 'demo', title: 'Demo', updated_at_ms: Date.now() });",
    '  });',
    "  ctx.host.querySelector('[data-action=\"follow-up\"]')?.addEventListener('click', () => {",
    "    ctx.commandBus.dispatch(buildFollowUpCommand({ id: 'demo', title: 'Demo', updated_at_ms: 1 }));",
    '  });',
    '  return () => { ctx.host.innerHTML = ""; };',
    '}',
    '',
  ].join('\n');
}

function writeInstalledModule(root, moduleId, overrides = {}) {
  const collectionName = `${moduleId}_records`;
  const dir = join(root, 'runtime/business-os/installed-modules', moduleId);
  mkdirSync(join(dir, 'locales'), { recursive: true });
  mkdirSync(join(dir, 'tests'), { recursive: true });
  mkdirSync(join(dir, 'core'), { recursive: true });
  writeJson(join(dir, 'module.json'), {
    id: moduleId,
    title: `${moduleId} Workbench`,
    description: `${moduleId} records and CTOX chat task follow-up.`,
    version: '0.1.0',
    entry: `installed-modules/${moduleId}/index.html`,
    install_scope: 'installed',
    icon: 'icon.svg',
    collections: ['business_commands', collectionName],
    launch_kind: 'desktop-app',
    layout: {
      shell: 'windowed',
      left: 'List',
      center: 'Details',
      default_width: 960,
      default_height: 680,
      min_width: 640,
      min_height: 480,
    },
    presentation: {
      default_mode: 'window',
      supported_modes: ['window', 'maximized', 'focus'],
      initial_size: { width: 960, height: 680 },
      minimum_size: { width: 640, height: 480 },
      multi_instance: false,
      auto_restore: false,
    },
    tags: ['business-os', moduleId, 'workflow'],
    ...overrides.manifest,
  });
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
    ...overrides.schemaDocument,
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
  writeFileSync(join(dir, 'core/automation.mjs'), overrides.automationJs || [
    'export function buildFollowUpCommand(record = {}) {',
    '  return {',
    `    module: '${moduleId}',`,
    "    command_type: 'business_os.chat.task',",
    "    record_id: record.id || 'demo',",
    '    payload: {',
    "      title: `Review ${record.title || 'record'}`,",
    "      instruction: `Review ${record.title || 'record'} and continue the normal CTOX workflow.`,",
    '      record_snapshot: record,',
    '    },',
    `    client_context: { source: '${moduleId}', collection: '${collectionName}' },`,
    '  };',
    '}',
    '',
  ].join('\n'));
  writeFileSync(join(dir, 'core/records.mjs'), overrides.recordsJs || [
    'export function visibleRecords(records = []) {',
    '  return records.filter((record) => !record.is_deleted);',
    '}',
    '',
    'export function summarizeRecords(records = []) {',
    '  return { total: visibleRecords(records).length };',
    '}',
    '',
  ].join('\n'));
  writeFileSync(join(dir, 'index.html'), overrides.indexHtml || [
    '<main class="validator-module ctox-pane">',
    '  <header class="ctox-pane-header">',
    '    <span class="ctox-pane-icon" aria-hidden="true"></span>',
    '    <h2>Records</h2>',
    '  </header>',
    '  <section class="ctox-fields" data-list></section>',
    '  <button class="ctox-button" type="button" data-action="create-record">Create record</button>',
    '  <button class="ctox-button" type="button" data-action="follow-up">Follow up</button>',
    '</main>',
    '',
  ].join('\n'));
  writeFileSync(join(dir, 'index.css'), overrides.indexCss || [
    '.validator-module {',
    '  display: grid;',
    '  gap: 12px;',
    '  min-height: 100%;',
    '  background: var(--surface);',
      '  color: var(--text);',
    '}',
    '',
  ].join('\n'));
  writeFileSync(join(dir, 'index.js'), overrides.indexJs || installedIndexJs(moduleId, collectionName));
  writeFileSync(join(dir, 'icon.svg'), '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"></svg>\n');
  writeJson(join(dir, 'locales/de.json'), { title: moduleId });
  writeJson(join(dir, 'locales/en.json'), { title: moduleId });
  writeFileSync(join(dir, 'tests/basic.test.mjs'), overrides.testJs || [
    "import assert from 'node:assert/strict';",
    "import { buildFollowUpCommand } from '../core/automation.mjs';",
    "import { summarizeRecords, visibleRecords } from '../core/records.mjs';",
    "const record = { id: 'demo', title: 'Demo', updated_at_ms: 1 };",
    "assert.equal(visibleRecords([record]).length, 1);",
    "assert.equal(summarizeRecords([record]).total, 1);",
    'const command = buildFollowUpCommand(record);',
    "assert.equal(command.command_type, 'business_os.chat.task');",
    "assert.deepEqual(command.payload.record_snapshot, record);",
    '',
  ].join('\n'));
  return dir;
}

function writeSourceModule(root, moduleId, overrides = {}) {
  const collectionName = `${moduleId}_records`;
  const dir = join(root, 'src/apps/business-os/modules', moduleId);
  mkdirSync(join(dir, 'locales'), { recursive: true });
  mkdirSync(join(dir, 'tests'), { recursive: true });
  const manifest = {
    id: moduleId,
    title: moduleId,
    version: '0.1.0',
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
  writeFileSync(join(dir, 'schema.js'), overrides.schemaJs || `export const collections = { ${collectionName}: { version: 0, primaryKey: 'id', type: 'object', properties: { id: { type: 'string', maxLength: 120 } } } };\n`);
  writeFileSync(join(dir, 'index.html'), '<main class="source-module">Ready</main>\n');
  writeFileSync(join(dir, 'index.css'), '.source-module { display: block; }\n');
  writeFileSync(join(dir, 'index.js'), overrides.indexJs || 'export async function mount(ctx) { ctx.host.textContent = "Ready"; return () => { ctx.host.textContent = ""; }; }\n');
  writeFileSync(join(dir, 'icon.svg'), '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"></svg>\n');
  writeJson(join(dir, 'locales/de.json'), { title: moduleId });
  writeJson(join(dir, 'locales/en.json'), { title: moduleId });
  writeFileSync(join(dir, 'tests/basic.test.mjs'), "import assert from 'node:assert/strict';\nassert.equal(1 + 1, 2);\n");
  if (overrides.registry !== false) {
    writeJson(join(root, 'src/apps/business-os/modules/registry.json'), {
      modules: [{
        id: moduleId,
        title: moduleId,
        entry: manifest.entry,
        install_scope: manifest.install_scope,
        collections: manifest.collections,
      }],
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
  writeInstalledModule(root, 'missingmigration', {
    collections: {
      missingmigration_records: {
        version: 1,
        primaryKey: 'id',
        type: 'object',
        properties: {
          id: { type: 'string', maxLength: 120 },
          title: { type: 'string' },
          updated_at_ms: { type: 'number' },
        },
        required: ['id', 'title', 'updated_at_ms'],
      },
    },
  });
  const run = runValidator(root, 'missingmigration', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /requires migration_strategies\.missingmigration_records\.1/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'paneshell', {
    manifest: { layout: { shell: 'pane', left: 'Kontext', center: 'Workbench', right: 'Themen' } },
  });
  const run = runValidator(root, 'paneshell', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /layout\.shell must be windowed/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'windowedgood', {
    manifest: {
      layout: {
        shell: 'windowed',
        launch_kind: 'desktop-app',
        default_width: 960,
        default_height: 680,
        min_width: 640,
        min_height: 480,
      },
      launch_kind: 'desktop-app',
    },
  });
  const run = runValidator(root, 'windowedgood', '--installed');
  assert.equal(run.status, 0, `${run.stderr}\n${run.stdout}`);
  assert.match(run.stdout, /validation OK: windowedgood \(installed mode\)/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'nothemetokens', {
    indexCss: '.validator-module { display: grid; gap: 12px; background: #101820; color: #ffffff; }\n',
  });
  const run = runValidator(root, 'nothemetokens', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /must use Business OS surface tokens/);
  assert.match(run.stderr, /must use Business OS text tokens/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'forcedscheme', {
    indexCss: '.validator-module { color-scheme: dark; background: var(--surface); color: var(--text); }\n',
  });
  const run = runValidator(root, 'forcedscheme', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /must not force color-scheme/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'insertadjacent', {
    indexJs: installedIndexJs('insertadjacent', 'insertadjacent_records').replace(
      "  ctx.host.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());",
      "  ctx.host.insertAdjacentHTML('beforeend', await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text()));",
    ),
  });
  const run = runValidator(root, 'insertadjacent', '--installed');
  assert.equal(run.status, 0, `${run.stderr}\n${run.stdout}`);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'namespacedaction', {
    indexHtml: [
      '<main class="validator-module ctox-pane">',
      '  <section class="ctox-fields" data-list></section>',
      '  <button class="ctox-button" type="button" data-demo-action="new">New record</button>',
      '</main>',
      '',
    ].join('\n'),
    indexJs: installedIndexJs('namespacedaction', 'namespacedaction_records', [
      "  ctx.host.querySelector('[data-demo-action=\"new\"]')?.addEventListener('click', () => {",
      "    records?.upsert?.({ id: 'demo-2', title: 'Demo 2', updated_at_ms: Date.now() });",
      '  });',
    ]),
  });
  const run = runValidator(root, 'namespacedaction', '--installed');
  assert.equal(run.status, 0, `${run.stderr}\n${run.stdout}`);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'submitsave', {
    indexHtml: [
      '<main class="validator-module ctox-pane">',
      '  <section class="ctox-fields" data-list></section>',
      '  <button class="ctox-button" type="button" data-action="create-record">Create record</button>',
      '  <form data-editor>',
      '    <input class="ctox-input" name="title" value="Demo">',
      '    <button class="ctox-button" type="submit" data-action="save-item">Save record</button>',
      '  </form>',
      '  <button class="ctox-button" type="button" data-action="follow-up">Follow up</button>',
      '</main>',
      '',
    ].join('\n'),
    indexJs: installedIndexJs('submitsave', 'submitsave_records', [
      "  ctx.host.querySelector('[data-editor]')?.addEventListener('submit', (event) => {",
      '    event.preventDefault();',
      "    records?.upsert?.({ id: 'submitted', title: 'Submitted', updated_at_ms: Date.now() });",
      '  });',
    ]),
  });
  const run = runValidator(root, 'submitsave', '--installed');
  assert.equal(run.status, 0, `${run.stderr}\n${run.stdout}`);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'buttonsave', {
    indexHtml: [
      '<main class="validator-module">',
      '  <section data-list></section>',
      '  <button type="button" data-action="create-record">Create record</button>',
      '  <button type="button" data-action="save-item">Save record</button>',
      '  <button type="button" data-action="follow-up">Follow up</button>',
      '</main>',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'buttonsave', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /data-action="save-item"/);
}

{
  const root = makeWorkspace();
  const indexJs = installedIndexJs('shellpreload', 'shellpreload_records')
    .replace(
      "  ctx.host.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());\n",
      "  const root = ctx.host.querySelector('[data-root]') || ctx.host;\n",
    )
    .replaceAll('ctx.host.querySelector', 'root.querySelector');
  writeInstalledModule(root, 'shellpreload', {
    indexHtml: [
      '<main class="validator-module" data-root>',
      '  <section data-list></section>',
      '  <button type="button" data-action="create-record">Create record</button>',
      '  <button type="button" data-action="follow-up">Follow up</button>',
      '</main>',
      '',
    ].join('\n'),
    indexJs,
  });
  const run = runValidator(root, 'shellpreload', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /Business OS shell does not preload runtime module index\.html/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'bench_inventory_rfix', {
    manifest: { collections: ['business_commands', 'bench_inventory_rfix_records', 'bench_inventory_items'] },
    collections: {
      bench_inventory_items: {
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
    },
    schemaJs: [
      'export const collections = {',
      '  bench_inventory_rfix_records: {',
      '    version: 0,',
      "    primaryKey: 'id',",
      "    type: 'object',",
      "    properties: { id: { type: 'string', maxLength: 120 }, title: { type: 'string' }, updated_at_ms: { type: 'number' } },",
      "    required: ['id', 'title', 'updated_at_ms'],",
      '  },',
      '  bench_inventory_items: {',
      '    version: 0,',
      "    primaryKey: 'id',",
      "    type: 'object',",
      "    properties: { id: { type: 'string', maxLength: 120 }, title: { type: 'string' }, updated_at_ms: { type: 'number' } },",
      "    required: ['id', 'title', 'updated_at_ms'],",
      '  },',
      '};',
      '',
    ].join('\n'),
    indexJs: installedIndexJs('bench_inventory_rfix', 'bench_inventory_items'),
  });
  const run = runValidator(root, 'bench_inventory_rfix', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /module\.json collection bench_inventory_items must be scoped to module id bench_inventory_rfix/);
  assert.match(run.stderr, /collections\.schema\.json collection bench_inventory_items must be scoped to module id bench_inventory_rfix/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'aliasautomation', {
    indexJs: [
      "import { buildFollowUpCommand } from './core/automation.mjs';",
      'export async function mount(ctx) {',
      "  ctx.host.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());",
      "  const records = ctx.db.collection('aliasautomation_records');",
      '  const state = { ctx, records };',
      "  ctx.host.querySelector('[data-action=\"create-record\"]')?.addEventListener('click', () => {",
      "    state.records?.upsert?.({ id: 'demo', title: 'Demo', updated_at_ms: Date.now() });",
      '  });',
      "  ctx.host.querySelector('[data-action=\"follow-up\"]')?.addEventListener('click', async () => {",
      '    const dispatch = state.ctx?.commandBus?.dispatch;',
      "    if (typeof dispatch === 'function') {",
      "      await dispatch(buildFollowUpCommand({ id: 'demo', title: 'Demo', updated_at_ms: 1 }));",
      '    }',
      '  });',
      '  return () => { ctx.host.innerHTML = ""; };',
      '}',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'aliasautomation', '--installed');
  assert.equal(run.status, 0, `${run.stderr}\n${run.stdout}`);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'legacycollections', {
    indexJs: [
      "import { buildFollowUpCommand } from './core/automation.mjs';",
      'export async function mount(ctx) {',
      "  ctx.host.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());",
      "  const records = ctx.db.collections?.legacycollections_records;",
      "  ctx.host.querySelector('[data-action=\"create-record\"]')?.addEventListener('click', () => records?.upsert?.({ id: 'demo', title: 'Demo', updated_at_ms: Date.now() }));",
      "  ctx.host.querySelector('[data-action=\"follow-up\"]')?.addEventListener('click', () => ctx.commandBus.dispatch(buildFollowUpCommand({ id: 'demo', title: 'Demo', updated_at_ms: 1 })));",
      '  return () => { ctx.host.innerHTML = ""; };',
      '}',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'legacycollections', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /uses legacy ctx\.db\.collections fallback/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'bracketcollection', {
    indexJs: [
      "import { buildFollowUpCommand } from './core/automation.mjs';",
      'export async function mount(ctx) {',
      "  ctx.host.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());",
      "  const name = 'bracketcollection_records';",
      '  const records = ctx.db[name];',
      "  ctx.host.querySelector('[data-action=\"create-record\"]')?.addEventListener('click', () => records?.upsert?.({ id: 'demo', title: 'Demo', updated_at_ms: Date.now() }));",
      "  ctx.host.querySelector('[data-action=\"follow-up\"]')?.addEventListener('click', () => ctx.commandBus.dispatch(buildFollowUpCommand({ id: 'demo', title: 'Demo', updated_at_ms: 1 })));",
      '  return () => { ctx.host.innerHTML = ""; };',
      '}',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'bracketcollection', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /uses bracket ctx\.db\[\.\.\.\] collection access/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'directproperty', {
    indexJs: [
      "import { buildFollowUpCommand } from './core/automation.mjs';",
      'export async function mount(ctx) {',
      "  ctx.host.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());",
      '  const records = ctx.db.directproperty_records;',
      "  ctx.host.querySelector('[data-action=\"create-record\"]')?.addEventListener('click', () => records?.upsert?.({ id: 'demo', title: 'Demo', updated_at_ms: Date.now() }));",
      "  ctx.host.querySelector('[data-action=\"follow-up\"]')?.addEventListener('click', () => ctx.commandBus.dispatch(buildFollowUpCommand({ id: 'demo', title: 'Demo', updated_at_ms: 1 })));",
      '  return () => { ctx.host.innerHTML = ""; };',
      '}',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'directproperty', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /uses direct ctx\.db\.directproperty_records access/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'registerschemas', {
    indexJs: [
      "import { buildFollowUpCommand } from './core/automation.mjs';",
      'export async function mount(ctx) {',
      "  ctx.host.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());",
      "  ctx.db.registerSchemas?.({ registerschemas_records: {} });",
      "  const records = ctx.db.collection('registerschemas_records');",
      "  ctx.host.querySelector('[data-action=\"create-record\"]')?.addEventListener('click', () => records?.upsert?.({ id: 'demo', title: 'Demo', updated_at_ms: Date.now() }));",
      "  ctx.host.querySelector('[data-action=\"follow-up\"]')?.addEventListener('click', () => ctx.commandBus.dispatch(buildFollowUpCommand({ id: 'demo', title: 'Demo', updated_at_ms: 1 })));",
      '  return () => { ctx.host.innerHTML = ""; };',
      '}',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'registerschemas', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /calls ctx\.db\.registerSchemas from app code/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'cachedfacade', {
    indexJs: [
      "import { buildFollowUpCommand } from './core/automation.mjs';",
      'export async function mount(ctx) {',
      "  ctx.host.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());",
      '  const db = ctx.db;',
      "  const records = db.collection('cachedfacade_records');",
      "  ctx.host.querySelector('[data-action=\"create-record\"]')?.addEventListener('click', () => records?.upsert?.({ id: 'demo', title: 'Demo', updated_at_ms: Date.now() }));",
      "  ctx.host.querySelector('[data-action=\"follow-up\"]')?.addEventListener('click', () => ctx.commandBus.dispatch(buildFollowUpCommand({ id: 'demo', title: 'Demo', updated_at_ms: 1 })));",
      '  return () => { ctx.host.innerHTML = ""; };',
      '}',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'cachedfacade', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /caches the ctx\.db facade in db/);
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
  writeSourceModule(root, 'sourcecore', {
    manifest: {
      install_scope: 'core',
      layout: {
        shell: 'full-workspace',
        icon_svg: '<svg viewBox="0 0 24 24"></svg>',
        left: 'Shell controls',
        center: 'Core workbench',
      },
    },
    collections: {
      business_commands: {
        version: 0,
        primaryKey: 'id',
        type: 'object',
        properties: { id: { type: 'string' } },
        required: ['id'],
      },
    },
    schemaJs: [
      'export const collections = {',
      "  business_commands: { version: 0, primaryKey: 'id', type: 'object', properties: { id: { type: 'string' } } },",
      "  sourcecore_records: { version: 0, primaryKey: 'id', type: 'object', properties: { id: { type: 'string', maxLength: 120 } } },",
      '};',
      '',
    ].join('\n'),
    indexJs: [
      'export async function mount(ctx) {',
      '  ctx.host.textContent = "Ready";',
      '  localStorage.setItem("sourcecore-layout", "1");',
      "  window.dispatchEvent(new CustomEvent('ctox-business-os-chat-submit', { detail: { text: 'ready' } }));",
      '  return () => { ctx.host.textContent = ""; };',
      '}',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'sourcecore', '--source');
  assert.equal(run.status, 0, `${run.stderr}\n${run.stdout}`);
  assert.match(run.stdout, /validation OK: sourcecore \(source mode\)/);
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
      source: 'local',
      version: 'v1',
      layout: { shell: 'full-workspace', left: 'List', center: 'Details', right: 'Inspector', right_resizer: false },
      store: { source_path: 'modules/badmanifest', distribution: 'ctox-repo-module', installable: true },
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
  assert.match(run.stderr, /module\.json version must be SemVer x\.y\.z without a v prefix/);
  assert.match(run.stderr, /module\.json source=local is a source\/store module manifest field/);
  assert.match(run.stderr, /module\.json store\.source_path must be installed-modules\/badmanifest/);
  assert.match(run.stderr, /module\.json store\.distribution must be ctox-runtime-installed-module/);
  assert.match(run.stderr, /module\.json store\.installable must not be true/);
  assert.match(run.stderr, /layout\.right requires layout\.third_pane_justification/);
  assert.match(run.stderr, /module\.json layout\.right_resizer is forbidden/);
  assert.match(run.stderr, /schema\.js exports shell-registered collection key business_commands/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'zeroversion', { manifest: { version: '0.0.0' } });
  const run = runValidator(root, 'zeroversion', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /module\.json version 0\.0\.0 is not a valid Business OS app work version/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'wrongicon', {
    manifest: { icon_svg: '<svg></svg>' },
  });
  const run = runValidator(root, 'wrongicon', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /module\.json inline icon fields are forbidden/);
  assert.match(run.stderr, /module\.json must not embed inline SVG markup/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'missingicon', {
    manifest: { icon: '' },
  });
  const run = runValidator(root, 'missingicon', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /module\.json icon must be icon\.svg/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'remoteicon', {
    manifest: { icon: 'icon.svg', icon_url: 'https://example.test/icon.svg' },
  });
  const run = runValidator(root, 'remoteicon', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /module\.json icon_url is forbidden/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'missingimport', {
    indexJs: installedIndexJs('missingimport', 'missingimport_records').replace(
      "import { buildFollowUpCommand } from './core/automation.mjs';",
      "import { buildFollowUpCommand } from './core/automation.mjs';\nimport { missingThing } from './core/missing.mjs';\nvoid missingThing;",
    ),
  });
  const run = runValidator(root, 'missingimport', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /relative import \.\/core\/missing\.mjs does not exist/);
}

{
  const root = makeWorkspace();
  const dir = writeInstalledModule(root, 'namedimportmismatch', {
    indexJs: installedIndexJs('namedimportmismatch', 'namedimportmismatch_records').replace(
      "import { buildFollowUpCommand } from './core/automation.mjs';",
      "import { buildFollowUpCommand } from './core/automation.mjs';\nimport { createRecord } from './core/records.mjs';\nvoid createRecord;",
    ),
  });
  writeFileSync(join(dir, 'core/records.mjs'), 'export function visibleRecords(records = []) { return records; }\nexport function summarizeRecords(records = []) { return { total: records.length }; }\n');
  const run = runValidator(root, 'namedimportmismatch', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /does not provide an export named `createRecord`/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'deadbutton', {
    indexHtml: '<main class="validator-module"><button type="button" data-action="create-record">Create record</button><button type="button" data-action="follow-up">Follow up</button><button type="button" data-action="bulk-follow-up">Bulk follow up</button></main>\n',
  });
  const run = runValidator(root, 'deadbutton', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /data-action="bulk-follow-up" but index\.js has no visible handler/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'namespaceddeadbutton', {
    indexHtml: '<main class="validator-module"><button type="button" data-demo-action="create-record">Create record</button><button type="button" data-demo-action="bulk-follow-up">Bulk follow up</button></main>\n',
    indexJs: installedIndexJs('namespaceddeadbutton', 'namespaceddeadbutton_records').replaceAll('data-action', 'data-demo-action'),
  });
  const run = runValidator(root, 'namespaceddeadbutton', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /data-action="bulk-follow-up" but index\.js has no visible handler/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'nocreate', {
    indexHtml: '<main class="validator-module"><button type="button" data-action="follow-up">Follow up</button></main>\n',
    indexJs: [
      "import { buildFollowUpCommand } from './core/automation.mjs';",
      'export async function mount(ctx) {',
      "  ctx.host.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());",
      "  const records = ctx.db.collection('nocreate_records');",
      '  void records;',
      "  ctx.host.querySelector('[data-action=\"follow-up\"]')?.addEventListener('click', () => ctx.commandBus.dispatch(buildFollowUpCommand({ id: 'demo', title: 'Demo', updated_at_ms: 1 })));",
      '  return () => { ctx.host.innerHTML = ""; };',
      '}',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'nocreate', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /must expose a primary create action/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'hiddenmodal', {
    indexHtml: [
      '<main class="validator-module">',
      '  <button type="button" data-action="create-record">Create record</button>',
      '  <button type="button" data-action="follow-up">Follow up</button>',
      '  <div class="hiddenmodal-modal" hidden>Modal</div>',
      '</main>',
      '',
    ].join('\n'),
    indexCss: '.validator-module { display: grid; }\n.hiddenmodal-modal { position: fixed; inset: 0; display: flex; }\n',
  });
  const run = runValidator(root, 'hiddenmodal', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /hidden modal \.hiddenmodal-modal has a display rule but no CSS rule/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'hiddenmodalok', {
    indexHtml: [
      '<main class="validator-module ctox-pane">',
      '  <button class="ctox-button" type="button" data-action="create-record">Create record</button>',
      '  <button class="ctox-button" type="button" data-action="follow-up">Follow up</button>',
      '  <div class="hiddenmodalok-modal" hidden>Modal</div>',
      '</main>',
      '',
    ].join('\n'),
    indexCss: '.validator-module { display: grid; background: var(--surface); color: var(--text); }\n.hiddenmodalok-modal { position: fixed; inset: 0; display: flex; background: var(--surface-2); color: var(--text); }\n.hiddenmodalok-modal[hidden] { display: none; }\n',
  });
  const run = runValidator(root, 'hiddenmodalok', '--installed');
  assert.equal(run.status, 0, `${run.stderr}\n${run.stdout}`);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'schemaversiondrift', {
    collections: {
      schemaversiondrift_records: {
        version: 1,
        primaryKey: 'id',
        type: 'object',
        properties: {
          id: { type: 'string', maxLength: 120 },
          title: { type: 'string' },
          updated_at_ms: { type: 'number' },
        },
        required: ['id', 'title', 'updated_at_ms'],
      },
    },
    schemaJs: [
      'export const collections = {',
      '  schemaversiondrift_records: {',
      "    version: 0, primaryKey: 'id', type: 'object',",
      "    properties: { id: { type: 'string', maxLength: 120 }, title: { type: 'string' }, updated_at_ms: { type: 'number' } },",
      "    required: ['id', 'title', 'updated_at_ms'],",
      '  },',
      '};',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'schemaversiondrift', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /schema\.js collection schemaversiondrift_records version does not match collections\.schema\.json/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'recordtypemismatch', {
    collections: {
      recordtypemismatch_records: {
        version: 0,
        primaryKey: 'id',
        type: 'object',
        additionalProperties: true,
        properties: {
          id: { type: 'string', maxLength: 120 },
          title: { type: 'string' },
          start_date: { type: 'string' },
          updated_at_ms: { type: 'number' },
        },
        required: ['id', 'title', 'updated_at_ms'],
      },
    },
    schemaJs: [
      'export const collections = {',
      '  recordtypemismatch_records: {',
      '    version: 0,',
      "    primaryKey: 'id',",
      "    type: 'object',",
      '    additionalProperties: true,',
      "    properties: { id: { type: 'string', maxLength: 120 }, title: { type: 'string' }, start_date: { type: 'string' }, updated_at_ms: { type: 'number' } },",
      "    required: ['id', 'title', 'updated_at_ms'],",
      '  },',
      '};',
      '',
    ].join('\n'),
    recordsJs: [
      'export function visibleRecords(records = []) { return records.filter((record) => !record._deleted); }',
      'export function summarizeRecords(records = []) { return { total: visibleRecords(records).length }; }',
      'export function normalizeRecord(input = {}, { nowMs }) {',
      '  return {',
      "    id: input.id || 'demo',",
      "    title: input.title || 'Demo',",
      '    start_date: Date.parse(input.start_date || "2026-01-02"),',
      '    updated_at_ms: nowMs,',
      '  };',
      '}',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'recordtypemismatch', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /core\/records\.mjs normalizeRecord returns start_date as number, but collections\.schema\.json declares string/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'domainaction', {
    indexHtml: '<main class="validator-module ctox-pane"><button class="ctox-button" type="button" data-action="create-record">Create record</button><button class="ctox-button" type="button" data-action="follow-up">Follow up</button><button class="ctox-button" type="button" data-action="restock">Restock</button></main>\n',
    indexJs: installedIndexJs('domainaction', 'domainaction_records', [
      "  ctx.host.querySelector('[data-action=\"restock\"]')?.addEventListener('click', () => {",
      "    ctx.commandBus.dispatch(buildFollowUpCommand({ id: 'sku-1', title: 'Safety gloves', updated_at_ms: 1 }));",
      '  });',
    ]),
  });
  const run = runValidator(root, 'domainaction', '--installed');
  assert.equal(run.status, 0, `${run.stderr}\n${run.stdout}`);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'duplicatefunction', {
    indexJs: [
      "import { buildFollowUpCommand } from './core/automation.mjs';",
      'function renderDetail(record) { return record?.title || ""; }',
      'export async function mount(ctx) {',
      "  ctx.host.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());",
      "  const records = ctx.db.collection('duplicatefunction_records');",
      '  void records;',
      '  function renderDetail() {',
      '    return renderDetail({ title: "Demo" });',
      '  }',
      '  void renderDetail;',
      "  ctx.host.querySelector('[data-action=\"create-record\"]')?.addEventListener('click', () => records?.upsert?.({ id: 'demo', title: 'Demo', updated_at_ms: Date.now() }));",
      "  ctx.host.querySelector('[data-action=\"follow-up\"]')?.addEventListener('click', () => ctx.commandBus.dispatch(buildFollowUpCommand({ id: 'demo', title: 'Demo', updated_at_ms: 1 })));",
      '  return () => { ctx.host.innerHTML = ""; };',
      '}',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'duplicatefunction', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /declares function renderDetail more than once/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'nosubmitcontrol', {
    indexJs: [
      "import { buildFollowUpCommand } from './core/automation.mjs';",
      'export async function mount(ctx) {',
      "  ctx.host.innerHTML = '<form data-form><input name=\"title\" required></form><button type=\"button\" data-action=\"create-record\">Create record</button><button type=\"button\" data-action=\"follow-up\">Follow up</button>';",
      "  const records = ctx.db.collection('nosubmitcontrol_records');",
      '  void records;',
      "  ctx.host.querySelector('[data-action=\"create-record\"]')?.addEventListener('click', () => {",
      "    ctx.host.querySelector('[data-form]').hidden = false;",
      '  });',
      "  ctx.host.querySelector('[data-form]')?.addEventListener('submit', (event) => {",
      '    event.preventDefault();',
      "    records?.upsert?.({ id: 'demo', title: 'Demo', updated_at_ms: Date.now() });",
      '  });',
      "  ctx.host.querySelector('[data-action=\"follow-up\"]')?.addEventListener('click', () => ctx.commandBus.dispatch(buildFollowUpCommand({ id: 'demo', title: 'Demo', updated_at_ms: 1 })));",
      '  return () => { ctx.host.innerHTML = ""; };',
      '}',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'nosubmitcontrol', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /wires a form submit handler but renders no visible submit\/save control/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'ticketautomation', {
    automationJs: [
      'export function buildFollowUpCommand(record = {}) {',
      '  return {',
      "    type: 'ctox.ticket.local.create',",
      "    command_type: 'ctox.ticket.local.create',",
      "    module: 'ticketautomation',",
      "    record_id: record.id || 'demo',",
      "    inbound_channel: 'ticketautomation',",
      '    payload: {',
      "      title: `Ticket: ${record.title || 'record'}`,",
      "      body: record.description || '',",
      "      status: 'open',",
      "      priority: 'normal',",
      "      source_module: 'ticketautomation',",
      "      source_collection: 'ticketautomation_records',",
      '    },',
      '  };',
      '}',
      '',
    ].join('\n'),
    testJs: [
      "import assert from 'node:assert/strict';",
      "import { buildFollowUpCommand } from '../core/automation.mjs';",
      "const command = buildFollowUpCommand({ id: 'demo', title: 'Demo' });",
      "assert.equal(command.type, 'ctox.ticket.local.create');",
      "assert.equal(command.command_type, 'ctox.ticket.local.create');",
      "assert.equal(command.payload.status, 'open');",
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'ticketautomation', '--installed');
  assert.equal(run.status, 0, `${run.stderr}\n${run.stdout}`);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'nodata', {
    indexJs: [
      "import { buildFollowUpCommand } from './core/automation.mjs';",
      'export async function mount(ctx) {',
      "  ctx.host.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());",
      "  ctx.host.querySelector('[data-action=\"follow-up\"]')?.addEventListener('click', () => ctx.commandBus.dispatch(buildFollowUpCommand({ id: 'demo', title: 'Demo' })));",
      '  return () => { ctx.host.innerHTML = ""; };',
      '}',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'nodata', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /must persist records through the shell-provided ctx\.db collection handle/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'noautomation', {
    indexJs: [
      'export async function mount(ctx) {',
      "  ctx.host.textContent = 'Ready';",
      "  const records = ctx.db.collection('noautomation_records');",
      '  void records;',
      '  return () => { ctx.host.textContent = ""; };',
      '}',
      '',
    ].join('\n'),
    automationJs: 'export function buildFollowUpCommand(record = {}) { return { type: "noop", payload: { record_snapshot: record } }; }\n',
  });
  const run = runValidator(root, 'noautomation', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /must dispatch at least one automation through ctx\.commandBus\.dispatch/);
  assert.match(run.stderr, /must include a supported automation command: business_os\.chat\.task or ctox\.ticket\.\*/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'aliasnotcalled', {
    indexJs: [
      "import { buildFollowUpCommand } from './core/automation.mjs';",
      'export async function mount(ctx) {',
      "  ctx.host.innerHTML = await fetch(new URL('./index.html', import.meta.url)).then((res) => res.text());",
      "  const records = ctx.db.collection('aliasnotcalled_records');",
      '  const dispatch = ctx.commandBus.dispatch;',
      '  void dispatch;',
      "  ctx.host.querySelector('[data-action=\"create-record\"]')?.addEventListener('click', () => {",
      "    records?.upsert?.({ id: 'demo', title: 'Demo', updated_at_ms: Date.now() });",
      '  });',
      "  ctx.host.querySelector('[data-action=\"follow-up\"]')?.addEventListener('click', () => {",
      "    buildFollowUpCommand({ id: 'demo', title: 'Demo', updated_at_ms: 1 });",
      '  });',
      '  return () => { ctx.host.innerHTML = ""; };',
      '}',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, 'aliasnotcalled', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /must dispatch at least one automation through ctx\.commandBus\.dispatch/);
  assert.doesNotMatch(run.stderr, /must include a supported automation command/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'badpatterns', {
    indexJs: installedIndexJs('badpatterns', 'badpatterns_records', [
      "  localStorage.setItem('badpatterns', '1');",
      "  await fetch('/api/business-os/records');",
      "  const element = React.createElement('div', null, 'bad');",
      "  const commands = ctx.db.collection('business_commands');",
      "  await commands.upsert({ id: 'cmd_direct' });",
      '  void element;',
    ]),
  });
  const run = runValidator(root, 'badpatterns', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /browser storage data path outside CTOX focus handoff/);
  assert.match(run.stderr, /Business OS HTTP data path/);
  assert.match(run.stderr, /React framework runtime/);
  assert.match(run.stderr, /direct business_commands write/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'fulldocumenthtml', {
    indexHtml: '<!doctype html><html><head><title>Bad</title><link rel="stylesheet" href="index.css"></head><body><main><button data-action="follow-up">Follow up</button></main></body></html>\n',
  });
  const run = runValidator(root, 'fulldocumenthtml', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /index\.html must be a shell fragment, not a full HTML document/);
  assert.match(run.stderr, /index\.html must not include document\/head resource tags/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'moduledependencies');
  mkdirSync(join(root, 'runtime/business-os/installed-modules/moduledependencies/lib'), { recursive: true });
  mkdirSync(join(root, 'runtime/business-os/installed-modules/moduledependencies/vendor'), { recursive: true });
  writeFileSync(join(root, 'runtime/business-os/installed-modules/moduledependencies/lib/math.mjs'), 'export const one = 1;\n');
  writeFileSync(join(root, 'runtime/business-os/installed-modules/moduledependencies/vendor/chart-lite.mjs'), 'export function chart() { return null; }\n');
  const localHelpersRun = runValidator(root, 'moduledependencies', '--installed');
  assert.equal(localHelpersRun.status, 0, `${localHelpersRun.stderr}\n${localHelpersRun.stdout}`);

  writeFileSync(join(root, 'runtime/business-os/installed-modules/moduledependencies/package.json'), '{"type":"module"}\n');
  mkdirSync(join(root, 'runtime/business-os/installed-modules/moduledependencies/node_modules'), { recursive: true });
  const run = runValidator(root, 'moduledependencies', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /forbidden module artifact .*package\.json/);
  assert.match(run.stderr, /unexpected installed-module root entry.*node_modules/);
}

{
  const root = makeWorkspace();
  writeInstalledModule(root, 'rootalias');
  writeFileSync(join(root, 'harness-module.json'), '{}\n');
  const run = runValidator(root, 'rootalias', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /root-level app artifact is forbidden: harness-module\.json/);
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
    testJs: "import assert from 'node:assert/strict';\nassert.equal('actual', 'expected');\n",
  });
  const run = runValidator(root, 'testbad', '--installed');
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, /module test failed: runtime\/business-os\/installed-modules\/testbad\/tests\/basic\.test\.mjs/);
}

{
  const root = makeWorkspace();
  const moduleId = 'runtimeok';
  const collection = `${moduleId}_records`;
  writeInstalledModule(root, moduleId, {
    manifest: {
      data_runtime: {
        version: 1,
        sync: 'realtime',
        scope: 'actor',
        actions: {
          save: {
            version: 1,
            input_schema: {
              type: 'object',
              required: ['id', 'title'],
              additionalProperties: false,
              properties: { id: { type: 'string' }, title: { type: 'string' } },
            },
            steps: [{
              name: 'save_record',
              op: 'upsert',
              collection,
              record: {
                id: { $input: 'id' },
                title: { $input: 'title' },
                actor_id: { $actor: 'id' },
                updated_at_ms: { $now_ms: true },
              },
            }],
          },
        },
      },
    },
    collections: {
      [collection]: {
        version: 0,
        primaryKey: 'id',
        type: 'object',
        properties: {
          id: { type: 'string', maxLength: 120 },
          title: { type: 'string' },
          actor_id: { type: 'string' },
          updated_at_ms: { type: 'number' },
        },
        required: ['id', 'title', 'actor_id', 'updated_at_ms'],
      },
    },
    schemaJs: [
      'export const collections = {',
      `  ${collection}: {`,
      "    version: 0, primaryKey: 'id', type: 'object',",
      "    properties: { id: { type: 'string', maxLength: 120 }, title: { type: 'string' }, actor_id: { type: 'string' }, updated_at_ms: { type: 'number' } },",
      "    required: ['id', 'title', 'actor_id', 'updated_at_ms'],",
      '  },',
      '};',
      '',
    ].join('\n'),
  });
  const run = runValidator(root, moduleId, '--installed', '--skip-tests', '--skip-node-check', '--json');
  assert.equal(run.status, 0, run.stderr || run.stdout);
  assert.equal(JSON.parse(run.stdout).checks.find((check) => check.name === 'data_runtime_v1').ok, true);
}

{
  const root = makeWorkspace();
  const moduleId = 'runtimebad';
  const collection = `${moduleId}_records`;
  writeInstalledModule(root, moduleId, {
    manifest: {
      data_runtime: {
        version: 1,
        actions: {
          unsafe: { steps: [{ op: 'sql', collection, sql: 'DELETE FROM everything' }] },
        },
      },
    },
  });
  const run = runValidator(root, moduleId, '--installed', '--skip-tests', '--skip-node-check', '--json');
  assert.notEqual(run.status, 0);
  const result = JSON.parse(run.stdout);
  assert.match(result.failures.join('\n'), /forbidden key sql/);
  assert.match(result.failures.join('\n'), /op is unsupported/);
}

console.log('[validate-app-module.test] OK');
