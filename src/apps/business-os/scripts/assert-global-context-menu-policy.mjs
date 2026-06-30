import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const appRoot = resolve(scriptDir, '..');

const appSource = readFileSync(resolve(appRoot, 'app.js'), 'utf8');
const matchingControlsSource = readFileSync(resolve(appRoot, 'modules/matching/ui/businessOsControls.js'), 'utf8');

const bypassMatch = appSource.match(/function\s+isCtoxContextMenuBypassTarget\s*\([^)]*\)\s*\{([\s\S]*?)\n\}/);
assert.ok(bypassMatch, 'global CTOX context menu bypass helper must exist');

const bypassBody = bypassMatch[1];
for (const selector of ['input', 'textarea', 'select']) {
  assert.doesNotMatch(
    bypassBody,
    new RegExp(`['"]${selector}['"]`),
    `${selector} elements must be handled by the global CTOX context menu, not legacy module menus`
  );
}

assert.doesNotMatch(
  matchingControlsSource,
  /^\s*initContextMenu\(\);\s*$/m,
  'Matching must not register its legacy local CTOX context menu at module load'
);

assert.match(
  appSource,
  /removeLegacyCtoxContextMenus\(\);/,
  'global CTOX context menu must clean up legacy local CTOX menus'
);

assert.doesNotMatch(
  appSource,
  /handleGlobalContextMenu[\s\S]{0,500}moduleUsesFullWorkspace/,
  'global CTOX context menu must not be limited to full-workspace modules'
);

console.log('Business OS global context menu policy OK');
