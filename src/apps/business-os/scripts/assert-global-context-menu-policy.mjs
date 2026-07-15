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

assert.match(appSource, /e\.key === 'ContextMenu'/, 'keyboard ContextMenu key must open the central menu');
assert.match(appSource, /e\.key === 'F10' && e\.shiftKey/, 'Shift+F10 must open the central menu');
assert.match(
  appSource,
  /target\.closest\('\[data-module-root\]'\).*?dataset\?\.moduleRoot/s,
  'windowed context attribution must derive the app from the clicked module root',
);
for (const field of [
  "schema_version: 'business-os-context-v2'",
  'window_instance_id:',
  'surface_id:',
  'pane_id:',
  'presentation_mode:',
  'entity,',
  'field: { path: fieldPath }',
  'ids: selectionIds',
  'x: Number.isFinite(pointer.clientX)',
]) {
  assert.ok(appSource.includes(field), `Context v2 must include ${field}`);
}
assert.match(
  appSource,
  /register:\s*\(element, descriptor = \{\}\)/,
  'ctx.contextActions must support explicit target registration',
);
for (const commandType of [
  'business_os.context.ask',
  'business_os.data.modify',
  'ctox.business_os.app.modify',
]) {
  assert.ok(appSource.includes(commandType), `${commandType} must use the Typed Command Bus`);
}

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

const menuMarkupMatch = appSource.match(/globalCtoxContextMenuEl\.innerHTML\s*=\s*`([\s\S]*?)`;\n\n\s*globalCtoxContextMenuEl\.hidden = false;/);
assert.ok(menuMarkupMatch, 'global CTOX context menu markup must be inspectable');
assert.doesNotMatch(
  menuMarkupMatch[1],
  /renderGlobalCtoxAgentScopeHtml/,
  'global CTOX context menu must not render the verbose CTOX access block by default'
);

const contextModeRenderMatch = appSource.includes('renderGlobalCtoxContextModeHtml')
  ? readFileSync(resolve(appRoot, 'shared/shell-permissions-ui.js'), 'utf8').match(/export function renderGlobalCtoxContextModeHtml[\s\S]*?\.join\(''\);\n}/)
  : null;
assert.ok(contextModeRenderMatch, 'global CTOX context mode renderer must exist');
assert.doesNotMatch(
  contextModeRenderMatch[0],
  /<small>/,
  'global CTOX context mode buttons must not repeat their label with an impact subtitle'
);

const contextModeBuilderMatch = readFileSync(resolve(appRoot, 'shared/shell-permissions-ui.js'), 'utf8')
  .match(/export function buildGlobalCtoxContextModes[\s\S]*?\n}\n\nexport function renderGlobalCtoxContextModeHtml/);
assert.ok(contextModeBuilderMatch, 'global CTOX context mode builder must exist');
for (const mode of ['note', 'mention', 'approval']) {
  assert.doesNotMatch(
    contextModeBuilderMatch[0],
    new RegExp(`value:\\s*['"]${mode}['"]`),
    `${mode} must not be a primary global CTOX context mode`
  );
}

console.log('Business OS global context menu policy OK');
