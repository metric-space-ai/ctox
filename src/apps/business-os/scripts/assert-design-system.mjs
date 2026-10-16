import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const appCss = readFileSync(join(root, 'app.css'), 'utf8');
const baseCss = readFileSync(join(root, 'shared/base.css'), 'utf8');
const lab = readFileSync(join(root, 'design-lab.html'), 'utf8');
const readModule = (id, file) => readFileSync(join(root, 'modules', id, file), 'utf8');

assert.match(appCss, /--panel-radius:\s*6px/);
assert.match(appCss, /--control-radius:\s*4px/);
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
// Paths are relative to src/apps/business-os. The coding agent's run surface moved from the Coding Agents app into
// the shell's code view (agent sidebar, Strang AGENT); the duty moved with the function.
for (const [id, rel] of [
  ['research', 'modules/research/index.js'],
  ['outbound', 'modules/outbound/index.js'],
  ['iot', 'modules/iot/index.js'],
  ['creator', 'modules/creator/index.html'],
  ['coding agent sidebar', 'v4/source/agent-panel.js'],
]) {
  const source = readFileSync(join(root, rel), 'utf8');
  const count = (source.match(/ctox-run-control/g) || []).length;
  assert.equal(count, 1, `${id} (${rel}) must expose exactly one signature Run Control source`);
}

for (const id of ['consent', 'credentials', 'esign', 'intake', 'interviews', 'nachweise', 'placements', 'submissions']) {
  // Record modules migrated to the shell pane-grammar layout (IA-Karte
  // rework): the canonical compact record workbench is now a `ctox-pane`
  // workbench surface carrying `ctox-compact-field` labelled fields under a
  // module-specific `*-workbench` container, replacing the retired
  // `ctox-record-workbench` class. Assert the substance, not the old class.
  const markup = readModule(id, 'index.html');
  assert.match(markup, /workbench/,
    `${id} must render a record workbench surface`);
  assert.match(markup, /ctox-compact-field/,
    `${id} must expose compact labelled fields`);
  assert.match(markup, /ctox-pane/,
    `${id} record workbench must sit on the shell pane-grammar layout`);
}

console.log('Business OS design-system contract OK');
