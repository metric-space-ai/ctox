import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { fileURLToPath } from 'node:url';

import { build } from 'esbuild';

const bundledModule = await build({
  entryPoints: [fileURLToPath(new URL('./app.js', import.meta.url))],
  bundle: true,
  format: 'esm',
  platform: 'browser',
  write: false,
});

const [{ text: bundledSource }] = bundledModule.outputFiles;
const {
  filterSourceFiles,
  formatSourceContent,
  isJavaScriptMime,
  normalizeModuleCatalog,
  normalizeSourceFiles,
  resolveMonacoBaseUrl,
  sourceEditorActionState,
} = await import(`data:text/javascript;base64,${Buffer.from(bundledSource).toString('base64')}`);

test('module catalog exposes editable Business OS apps once each', () => {
  const modules = normalizeModuleCatalog([
    { id: 'ctox', title: 'CTOX', editable: true },
    { id: 'ctox', title: 'Duplicate', editable: true },
    { id: 'hidden', title: 'Hidden', hidden: true },
    { id: 'locked', title: 'Locked', editable: false },
    { id: 'notes', title: 'Notizen' },
  ]);

  assert.deepEqual(modules.map((module) => module.id).sort(), ['ctox', 'notes']);
  assert.equal(modules.find((module) => module.id === 'notes').title, 'Notizen');
});

test('source file normalization removes deleted rows and sorts paths', () => {
  const files = normalizeSourceFiles([
    { path: 'z.css', content: '', is_deleted: false },
    { path: 'deleted.js', _deleted: true },
    { path: 'index.js', content: 'export {}' },
    { path: '', content: 'bad' },
  ]);

  assert.deepEqual(files.map((file) => file.path), ['index.js', 'z.css']);
  assert.equal(files[0].language, 'javascript');
  assert.equal(files[1].language, 'css');
  assert.equal(files[0].dirty, false);
});

test('file search matches path and language without mutating rows', () => {
  const files = normalizeSourceFiles([
    { path: 'index.js', language: 'javascript' },
    { path: 'styles/main.css', language: 'css' },
  ]);

  assert.deepEqual(filterSourceFiles(files, 'css').map((file) => file.path), ['styles/main.css']);
  assert.equal(filterSourceFiles(files, '').length, 2);
});

test('actions stay disabled until a dirty writable file is selected', () => {
  assert.deepEqual(sourceEditorActionState({}), {
    openApp: false,
    diff: false,
    format: false,
    revert: false,
    reload: false,
    save: false,
  });

  assert.deepEqual(sourceEditorActionState({
    moduleId: 'ctox',
    hasFile: true,
    dirty: true,
    readonly: false,
  }), {
    openApp: true,
    diff: true,
    format: true,
    revert: true,
    reload: true,
    save: true,
  });

  assert.equal(sourceEditorActionState({
    moduleId: 'ctox',
    hasFile: true,
    dirty: true,
    readonly: true,
  }).save, false);
  assert.equal(sourceEditorActionState({
    moduleId: 'ctox',
    hasFile: true,
    dirty: false,
    readonly: true,
  }).format, false);
});

test('monaco asset path resolves under the Business OS app bundle', () => {
  assert.equal(
    resolveMonacoBaseUrl('https://cto1.kunstmen.com/business-os/desktop-apps/code-editor/app.js?v=1'),
    'https://cto1.kunstmen.com/business-os/vendor/monaco/',
  );
  assert.equal(isJavaScriptMime('text/html; charset=utf-8'), false);
  assert.equal(isJavaScriptMime('application/javascript; charset=utf-8'), true);
});

test('formatter handles json and low-risk whitespace cleanup', () => {
  assert.equal(formatSourceContent('{"b":1,"a":2}', 'json'), '{\n  "b": 1,\n  "a": 2\n}\n');
  assert.equal(formatSourceContent('const value = 1;  \n\n', 'javascript'), 'const value = 1;\n');
});
