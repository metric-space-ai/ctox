import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const appSource = await readFile(new URL('../app.js', import.meta.url), 'utf8');
const desktopSource = await readFile(new URL('../modules/desktop/index.js', import.meta.url), 'utf8');

assert.match(appSource, /commandType:\s*'ctox\.module\.list_versions'[\s\S]*?until:\s*'terminal'/);
assert.match(appSource, /commandType:\s*'ctox\.module\.rollback_version'[\s\S]*?until:\s*'terminal'/);
assert.match(appSource, /BusinessOsPermissions\.AppsModify/);
assert.match(appSource, /BusinessOsPermissions\.AppsRollback/);
assert.match(appSource, /canViewModuleSource\(mod\)/);
assert.match(appSource, /document\.addEventListener\('keydown', keydown, true\)/);
assert.match(appSource, /document\.removeEventListener\('keydown', keydown, true\)/);
assert.match(appSource, /state\.eventBus\?\.on\?\.\('window:closing', closeMenuForWindow\)/);
assert.match(appSource, /state\.eventBus\?\.off\?\.\('window:closing', closingToken\)/);
assert.match(appSource, /event\.key === 'Escape'/);
assert.match(appSource, /\['ArrowDown', 'ArrowUp'\]\.includes\(event\.key\)/);
assert.match(appSource, /if \(busy\) return/);
assert.match(appSource, /workspaceContext:\s*\{ source: 'shell-v2-version-menu'/);
assert.match(appSource, /await openModuleSourceEditor\(mod\.id\)/);
assert.match(appSource, /historyAction\?\.addEventListener\('click', loadHistory\)/);
assert.match(appSource, /data-v2-version-retry/);
assert.match(appSource, /historyLoading = true/);
assert.match(appSource, /desktopAppTargetAvailable\('coding-agents'\)/);
assert.match(appSource, /canOpenCodingAgent = canCode && codingAgentAvailable/);
assert.match(appSource, /sourceMountPromise = null;[\s\S]*?renderIntegratedModuleSourceError/);
assert.match(appSource, /SHELL_INTEGRATED_TOOL_TIMEOUT_MS/);
assert.match(appSource, /getActionIcon: getRegisteredActionIcon/);
assert.match(appSource, /openDesktopApp,[\s\S]*?openBusinessChat,/);
assert.match(appSource, /'code-editor': \[[\s\S]*?'business_module_commits'[\s\S]*?'business_module_source_files'/);
assert.match(appSource, /maintenanceRemountModuleId = mod\.id/);
assert.match(appSource, /if \(wasActive\) resumeMaintenanceInterruptedModuleMount\(\)/);
assert.match(appSource, /mod\.id === 'desktop' && state\.maintenance\?\.active[\s\S]*?assertMaintenanceWriteAllowed\('desktop'\)/);
// The clock widget paints once while mounting, but its second-tick must not run
// behind a desktop that never finished: `ensureIcons` can reject during
// maintenance. The timer therefore lives in `startClockTimer`, which is called
// after the persistence step. Comparing the raw `setInterval` position instead
// measured where the closure is *written*, not when it runs.
assert.match(
  desktopSource,
  /startClockTimer = \(\) => \{\s*const clockInterval = setInterval\(updateClock, 1000\);/,
  'the desktop clock timer is created inside startClockTimer',
);
assert.ok(
  desktopSource.indexOf('await ensureIcons(iconsCollection, launcher);')
    < desktopSource.indexOf('startClockTimer?.();'),
  'desktop timers start only after maintenance-sensitive icon persistence succeeds',
);
assert.doesNotMatch(appSource, /<div><span>Knowledge<\/span>/);
assert.doesNotMatch(appSource, /<p>Knowledge wirklich auf Version/);

console.log('Business OS shell-v2 version/source/coding menu contract OK');
