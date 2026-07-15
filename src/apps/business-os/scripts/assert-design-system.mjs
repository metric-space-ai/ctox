import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const appCss = readFileSync(join(root, 'app.css'), 'utf8');
const baseCss = readFileSync(join(root, 'shared/base.css'), 'utf8');
const lab = readFileSync(join(root, 'design-lab.html'), 'utf8');
const readModule = (id, file) => readFileSync(join(root, 'modules', id, file), 'utf8');

assert.match(appCss, /--panel-radius:\s*4px/);
assert.match(appCss, /--control-radius:\s*3px/);
assert.match(appCss, /Operational Instrument contract/);
assert.match(baseCss, /\.ctox-run-control\s*\{/);
assert.match(baseCss, /\.ctox-action-strip\s*\{/);
assert.match(baseCss, /@container business-app-window/);
assert.match(baseCss, /prefers-reduced-motion:\s*reduce/);
for (const className of ['ctox-workspace', 'ctox-pane', 'ctox-action-strip', 'ctox-table', 'ctox-run-control']) {
  assert.ok(lab.includes(className), `Design Lab must render ${className}`);
}

// The accent-heavy Run Control is deliberately scarce: one source declaration
// per signature automation surface. Routine forms must use the compact
// workbench and ordinary buttons instead.
for (const [id, file] of [
  ['research', 'index.js'],
  ['outbound', 'index.js'],
  ['iot', 'index.js'],
  ['creator', 'index.html'],
  ['coding-agents', 'index.html'],
]) {
  const source = readModule(id, file);
  const count = (source.match(/ctox-run-control/g) || []).length;
  assert.equal(count, 1, `${id} must expose exactly one signature Run Control source`);
}

for (const id of ['consent', 'credentials', 'esign', 'intake', 'interviews', 'nachweise', 'placements', 'submissions']) {
  const markup = readModule(id, 'index.html');
  assert.match(markup, /ctox-record-workbench/,
    `${id} must use the canonical compact record workbench`);
  assert.match(markup, /ctox-compact-field/,
    `${id} must expose compact labelled fields`);
}

console.log('Business OS design-system contract OK');
