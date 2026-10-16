// node --test src/apps/business-os/v4/source/text-diff.test.mjs
// Patch übernehmen (Strang AGENT, §16.44 / Stub-Pin 8489b4e): unified diff mit 3 Kontextzeilen, Kontextprüfung.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { applyPatch, parsePatch, lineDiff } from './text-diff.js';

const BASE = 'a\nb\nc\nd\ne\n';
const PATCH = '--- a/x.js\n+++ b/x.js\n@@ -3,3 +3,4 @@\n c\n d\n e\n+agent\n';

test('Kontext-Patch auf unveränderter Basis', () => {
  assert.deepEqual(applyPatch({ 'x.js': BASE }, PATCH), { 'x.js': 'a\nb\nc\nd\ne\nagent\n' });
});
test('Nutzer fügt oben Zeilen ein → Patch landet trotzdem an der richtigen Stelle', () => {
  assert.deepEqual(applyPatch({ 'x.js': `neu1\nneu2\n${BASE}` }, PATCH), { 'x.js': 'neu1\nneu2\na\nb\nc\nd\ne\nagent\n' });
});
test('Kontext passt nicht (Nutzer hat eine Kontextzeile geändert) → PatchConflict, nie stumm daneben', () => {
  assert.throws(() => applyPatch({ 'x.js': 'a\nb\nc\nD\ne\n' }, PATCH), (e) => e.name === 'PatchConflict' && /Kontext passt nicht/.test(e.message));
});
test('Kontext mehrdeutig → PatchConflict', () => {
  const p = '--- a/y.js\n+++ b/y.js\n@@ -1,1 +1,2 @@\n x\n+neu\n';
  assert.throws(() => applyPatch({ 'y.js': 'q\nx\nz\nx\n' }, p), (e) => e.name === 'PatchConflict' && /mehrdeutig/.test(e.message));
});
test('Mehrere Hunks, Ersetzung mit Kontext', () => {
  const p = '--- a/z.js\n+++ b/z.js\n@@ -1,2 +1,2 @@\n-eins\n+EINS\n zwei\n@@ -5,2 +5,2 @@\n fuenf\n-sechs\n+SECHS\n';
  assert.deepEqual(applyPatch({ 'z.js': 'eins\nzwei\ndrei\nvier\nfuenf\nsechs\n' }, p), { 'z.js': 'EINS\nzwei\ndrei\nvier\nfuenf\nSECHS\n' });
});
test('Alter Kern (Null-Kontext-Einfügung) und neue/gelöschte Datei', () => {
  assert.deepEqual(applyPatch({ 'x.js': 'l1\nl2\n' }, '--- a/x.js\n+++ b/x.js\n@@ -2,0 +3,1 @@\n+agent\n'), { 'x.js': 'l1\nl2\nagent\n' });
  assert.deepEqual(applyPatch({}, '--- /dev/null\n+++ b/n.md\n@@ -0,0 +1,1 @@\n+hallo\n'), { 'n.md': 'hallo\n' });
  assert.deepEqual(applyPatch({ 'x.js': 'l1\n' }, '--- a/x.js\n+++ /dev/null\n@@ -1,1 +0,0 @@\n-l1\n'), { 'x.js': null });
});
test('Zeile, die mit „--- “ beginnt, beendet den Hunk nicht', () => {
  const p = '--- a/m.md\n+++ b/m.md\n@@ -1,1 +1,2 @@\n kopf\n+-- trenner\n';
  assert.equal(parsePatch(p)[0].hunks[0].new.at(-1), '-- trenner');
});
test('lineDiff: bei Gleichstand „−“ vor „+“', () => {
  assert.deepEqual(lineDiff('a\nb\n', 'a\nB\n').map((r) => r.op + r.text), [' a', '-b', '+B']);
});
