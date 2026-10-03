import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';

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
// behind a desktop whose local first paint failed. Icon repair is now detached
// from mount; its failure is handled there rather than rejecting mount. The
// timer must start after first paint and remain covered by mount cleanup.
assert.match(
  desktopSource,
  /startClockTimer = \(\) => \{\s*const clockInterval = setInterval\(updateClock, 1000\);/,
  'the desktop clock timer is created inside startClockTimer',
);
assert.ok(
  desktopSource.indexOf('await renderIcons();')
    < desktopSource.indexOf('startClockTimer?.();')
    && desktopSource.indexOf('startClockTimer?.();')
      < desktopSource.indexOf('const reconciliationTimer = setTimeout('),
  'desktop timers start only after local first paint and before detached icon repair',
);
assert.match(desktopSource, /cleanups\.push\(\(\) => clearInterval\(clockInterval\)\)/);
assert.match(desktopSource, /cleanups\.push\(\(\) => clearTimeout\(reconciliationTimer\)\)/);
assert.doesNotMatch(appSource, /<div><span>Knowledge<\/span>/);
assert.doesNotMatch(appSource, /<p>Knowledge wirklich auf Version/);

console.log('Business OS shell-v2 version/source/coding menu contract OK');

// Exercise the actual backend renderer, not a reimplemented status model.
const htmlSource = await readFile(new URL('../index.html', import.meta.url), 'utf8');
const cssSource = await readFile(new URL('../app.css', import.meta.url), 'utf8');
assert.equal((htmlSource.match(/\bdata-ctox-version(?:\s|>)/g) || []).length, 1,
  'the backend status has exactly one real DOM container');
assert.match(htmlSource, /data-ctox-version hidden>[\s\S]*?data-ctox-version-label>CTOX —<\/span>[\s\S]*?data-ctox-update-button hidden/);
assert.match(cssSource, /\.ctox-backend-version-line\s*\{\s*max-width:\s*240px;/);
const rendererDefinitions = ['renderShellCtoxVersion', 'platformDisplayVersion', 'platformBuildStamp']
  .map(name => {
    const definition = appSource.match(new RegExp('^function ' + name + '\\([^\\n]*\\) \\{[\\s\\S]*?^\\}', 'm'));
    assert.ok(definition, 'actual backend function is present: ' + name);
    return definition[0];
  }).join('\n');
const backendLabel = { textContent: 'stale' };
const backendButton = { hidden: false, disabled: false, textContent: '', title: '' };
const backendContainer = {
  hidden: false, title: 'stale',
  querySelector(selector) {
    if (selector === '[data-ctox-version-label]') return backendLabel;
    if (selector === '[data-ctox-update-button]') return backendButton;
    throw new Error('unexpected backend child selector: ' + selector);
  },
  removeAttribute(name) { assert.equal(name, 'title'); this.title = ''; },
};
let backendManager = true;
let backendCheck = null;
let backendRefreshes = 0;
const backendState = { ctoxHealth: null, ctoxUpdateInstallRunning: false, ctoxUpdateCheckRunning: false };
const backendContext = vm.createContext({
  els: { ctoxVersion: backendContainer }, state: backendState,
  sessionCanManageCtoxPlatform: () => backendManager,
  maybeRefreshCtoxUpdateCheck: () => { backendRefreshes += 1; },
  currentCtoxUpdateCheck: () => backendCheck,
  shellText: key => key,
  ctoxVersionTitle: () => 'actual native runtime metadata',
});
vm.runInContext(rendererDefinitions, backendContext);
const renderBackend = status => backendContext.renderShellCtoxVersion(status);
renderBackend(null);
assert.equal(backendContainer.hidden, false, 'admin sees explicit unavailable backend status');
assert.equal(backendLabel.textContent, 'CTOX —');
assert.match(backendContainer.title, /nicht verfügbar.*Sync/);
assert.equal(backendButton.hidden, true, 'no update action without native runtime version');
assert.equal(backendRefreshes, 0, 'missing metadata never starts an update check');
backendCheck = { update_available: false };
const nativePlatform = { runtime_settings: { platform: {
  version: '0.3.22', current_release: 'branch-main-20261003T110000Z',
} } };
renderBackend(nativePlatform);
assert.equal(backendLabel.textContent, 'CTOX v0.3.22 · Stand 03.10.2026',
  'visible backend version comes from native version and release stamp');
assert.equal(backendContainer.hidden, false);
assert.equal(backendButton.hidden, true);
backendCheck = { update_available: true, latest_release: '0.3.23' };
renderBackend(nativePlatform);
assert.equal(backendButton.hidden, false, 'actual update availability exposes the action');
assert.equal(backendButton.disabled, false);
assert.match(backendLabel.textContent, /v0\.3\.23 ctoxUpdateAvailable/);
backendState.ctoxUpdateInstallRunning = true;
renderBackend(nativePlatform);
assert.equal(backendButton.disabled, true);
assert.match(backendLabel.textContent, /ctoxUpdateInstalling/);
backendState.ctoxUpdateInstallRunning = false;
renderBackend(null);
assert.equal(backendLabel.textContent, 'CTOX —', 'lost runtime metadata clears the stale version');
assert.equal(backendButton.hidden, true, 'lost runtime metadata hides a stale update action');
backendManager = false;
renderBackend(nativePlatform);
assert.equal(backendContainer.hidden, true, 'non-manager keeps the existing visibility boundary');
assert.equal(backendContainer.title, '', 'non-manager does not inherit administrator metadata');
console.log('Backend DOM, unavailable/recovery, native release, update and permission rendering OK');
