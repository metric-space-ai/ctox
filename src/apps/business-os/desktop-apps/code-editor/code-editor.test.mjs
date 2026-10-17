// Editor-Bibliothek (Shell v4): reine Hilfsfunktionen + Quelltext-Wächter. Browser-Verhalten: tests/v4-api.test.mjs.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import {
  buildLineDiff, filterSourceFiles, formatSourceContent, isJavaScriptMime, normalizeSourceFiles,
  resolveMonacoBaseUrl, sourceEditorActionState, sourceModelPath,
} from './app.js';

const appSource = (await readFile(new URL('./app.js', import.meta.url), 'utf8')).split('\n').filter((l) => !/^\s*\/\//.test(l)).join('\n');

test('Bibliothek statt App: createCodeEditor exportiert, kein mount/ctx/Datenweg', () => {
  assert.match(appSource, /export async function createCodeEditor\(\{ root, files = \[\], onChange, readOnly = false/);
  assert.doesNotMatch(appSource, /export (async )?function mount\b/);
  assert.doesNotMatch(appSource, /\bctx\.|commandBus|registry\.json|until:|startCollection|setInterval|\?v=/);
  assert.doesNotMatch(appSource, /delay\(300\)|45000|45_000/);
});

test('source file normalization removes deleted rows and sorts paths', () => {
  const files = normalizeSourceFiles([
    { path: 'z.js', content: 'z' }, { path: ' a.css ', content: 'a' }, { path: 'gone.js', is_deleted: true }, { path: '' }, null,
  ]);
  assert.deepEqual(files.map((f) => f.path), ['a.css', 'z.js']);
  assert.equal(files[0].language, 'css');
  assert.equal(files[1].dirty, false);
});

test('file search matches path and language without mutating rows', () => {
  const files = normalizeSourceFiles([{ path: 'index.js' }, { path: 'index.css' }, { path: 'module.json' }]);
  assert.deepEqual(filterSourceFiles(files, 'css').map((f) => f.path), ['index.css']);
  assert.deepEqual(filterSourceFiles(files, 'JAVASCRIPT').map((f) => f.path), ['index.js']);
  assert.equal(filterSourceFiles(files, '').length, 3);
});

test('editor actions stay disabled without file, revert needs a dirty writable file', () => {
  assert.deepEqual(sourceEditorActionState({}), { canFormat: false, canRevert: false, canDiff: false });
  assert.deepEqual(sourceEditorActionState({ hasFile: true }), { canFormat: true, canRevert: false, canDiff: true });
  assert.deepEqual(sourceEditorActionState({ hasFile: true, dirty: true, readOnly: true }), { canFormat: false, canRevert: false, canDiff: true });
  assert.equal(sourceEditorActionState({ hasFile: true, dirty: true }).canRevert, true);
});

test('monaco kommt aus der Shell-Standardbibliothek ctox:lib/monaco@0 (SW-umgeschrieben), ohne Rückfallpfad', () => {
  assert.equal(resolveMonacoBaseUrl('/~a/shell@4.0.0/lib/monaco@0/', 'https://x.test'), 'https://x.test/~a/shell@4.0.0/lib/monaco@0/');
  assert.equal(resolveMonacoBaseUrl('ctox:lib/monaco@0/', 'https://x.test'), 'ctox:lib/monaco@0/');
  assert.match(appSource, /'ctox:lib\/monaco@0\/'/);
  assert.doesNotMatch(appSource, /vendor\/monaco/);
  assert.equal(isJavaScriptMime('text/javascript; charset=utf-8'), true);
  assert.equal(isJavaScriptMime('text/html'), false);
});

test('monaco model paths never create an authority-less double-slash URI', () => {
  assert.equal(sourceModelPath('', '/index.js'), '/unselected/index.js');
  assert.equal(sourceModelPath('notes/', 'a//b.js'), '/notes/a/b.js');
});

test('formatter handles json and low-risk whitespace cleanup', () => {
  assert.equal(formatSourceContent('{"a":1}', 'json'), '{\n  "a": 1\n}\n');
  assert.equal(formatSourceContent('a  \nb', 'javascript'), 'a\nb\n');
});

test('line diff counts added and removed lines', () => {
  const diff = buildLineDiff('a\nb\nc', 'a\nB\nc\nd');
  assert.equal(diff.added, 3);
  assert.equal(diff.removed, 2);
});
