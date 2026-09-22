import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { loadBusinessOsAppInventory } from '../scripts/business-os-app-inventory.mjs';
import {
  isShellSurfaceModule,
  launchesInWindow,
  resolvePresentation,
  resolveShellWindowContract,
  SHELL_WINDOW_CONTRACT,
  SHELL_WINDOW_GEOMETRY_CONTRACT,
  SHELL_SURFACE_MODULE_ID,
  usesLegacyWorkspace,
} from './presentation.js';
import {
  SHELL_WINDOW_CHROME_VERSION,
  SHELL_WINDOW_V2_CHROME_VERSION,
  SHELL_WINDOW_CONTROL_ACTIONS,
} from './window-manager.js';

const here = dirname(fileURLToPath(import.meta.url));
const businessOsRoot = resolve(here, '..');
const appSource = readFileSync(resolve(businessOsRoot, 'app.js'), 'utf8');
const appCss = readFileSync(resolve(businessOsRoot, 'app.css'), 'utf8');
const windowManagerSource = readFileSync(resolve(here, 'window-manager.js'), 'utf8');
const registry = JSON.parse(readFileSync(resolve(businessOsRoot, 'modules/registry.json'), 'utf8'));
const systemApps = JSON.parse(readFileSync(resolve(businessOsRoot, 'system-apps.json'), 'utf8'));

test('every registry app is classified as a shared-window app or the one shell surface', () => {
  const inventory = loadBusinessOsAppInventory();
  const modules = Array.isArray(registry?.modules) ? registry.modules : [];
  const shellSurfaces = modules.filter(isShellSurfaceModule);

  // The inventory helper verifies registry/source parity and exact counts; the
  // loop below deliberately touches every registry entry so a new app cannot
  // silently skip the windowing contract.
  assert.equal(modules.length, inventory.sourceApps.length);
  assert.deepEqual(shellSurfaces.map((mod) => mod.id), [SHELL_SURFACE_MODULE_ID]);
  assert.deepEqual(new Set(systemApps.apps), new Set(inventory.coreApps.map((app) => app.id)));

  for (const moduleDef of modules) {
    assert.ok(moduleDef?.id, 'every registry entry needs an id');
    if (isShellSurfaceModule(moduleDef)) {
      assert.equal(moduleDef.layout?.shell, 'full-workspace');
      assert.equal(launchesInWindow(moduleDef), false);
      assert.equal(usesLegacyWorkspace(moduleDef), true);
      continue;
    }

    assert.equal(moduleDef.launch_kind, 'desktop-app', `${moduleDef.id} must declare the app launch kind`);
    assert.equal(moduleDef.layout?.shell, 'windowed', `${moduleDef.id} must use the windowed shell contract`);
    assert.ok(moduleDef.presentation && typeof moduleDef.presentation === 'object');
    assert.ok(['window', 'maximized', 'focus'].includes(moduleDef.presentation.default_mode));
    for (const mode of ['window', 'maximized', 'focus']) {
      assert.ok(moduleDef.presentation.supported_modes.includes(mode), `${moduleDef.id} missing ${mode} support`);
    }
    assert.equal(launchesInWindow(moduleDef), true, `${moduleDef.id} must launch in the shared window shell`);
    assert.equal(usesLegacyWorkspace(moduleDef), false, `${moduleDef.id} must not use the workspace bypass`);
  }
});

test('the tenant shell resolves every windowed app to shell v2', () => {
  const inventory = loadBusinessOsAppInventory();
  // Assert the contract the tenant shell actually resolves, rather than a
  // stale raw registry field. Older registry snapshots intentionally omit the
  // explicit v2 marker while resolveShellWindowContract still fails closed to
  // the current shared v2 contract.
  const v2Modules = registry.modules.filter((moduleDef) => resolveShellWindowContract(moduleDef)?.contract === 'v2');
  const visibleWindowModules = registry.modules.filter((moduleDef) => (
    moduleDef.id !== SHELL_SURFACE_MODULE_ID && moduleDef.install_scope !== 'internal'
  ));
  const expectedVisibleWindowIds = inventory.sourceApps
    .filter((app) => app.id !== SHELL_SURFACE_MODULE_ID && app.installScope !== 'internal')
    .map((app) => app.id)
    .sort();
  assert.equal(visibleWindowModules.length, expectedVisibleWindowIds.length, 'the release gate must cover every visible app');
  assert.deepEqual(
    visibleWindowModules.map((moduleDef) => moduleDef.id).sort(),
    expectedVisibleWindowIds,
    'every non-internal source app must have one visible window entry',
  );
  assert.deepEqual(
    v2Modules.map((moduleDef) => moduleDef.id).sort(),
    visibleWindowModules.map((moduleDef) => moduleDef.id).sort(),
  );
  for (const moduleDef of visibleWindowModules) {
    const sourceManifest = JSON.parse(readFileSync(
      resolve(businessOsRoot, 'modules', moduleDef.id, 'module.json'),
      'utf8',
    ));
    const sourceShell = resolveShellWindowContract(sourceManifest);
    assert.equal(sourceShell?.contract, 'v2', `${moduleDef.id} source manifest must resolve to v2`);
    assert.ok(sourceShell?.geometryContract, `${moduleDef.id} source manifest needs a v2 geometry resolution`);
  }
  const knowledge = v2Modules.find((moduleDef) => moduleDef.id === 'knowledge');
  assert.equal(SHELL_WINDOW_CONTRACT, 'v2');
  assert.equal(SHELL_WINDOW_GEOMETRY_CONTRACT, 'business-os-v2-global-1');
  for (const moduleDef of registry.modules) {
    if (moduleDef.id === SHELL_SURFACE_MODULE_ID) {
      assert.equal(resolveShellWindowContract(moduleDef), null);
      continue;
    }
    const shell = resolveShellWindowContract(moduleDef);
    assert.equal(shell.contract, 'v2', `${moduleDef.id} must resolve to shell v2`);
    assert.ok(shell.geometryContract, `${moduleDef.id} needs a v2 geometry contract`);
  }
  assert.equal(
    resolveShellWindowContract({ id: 'runtime-old', layout: { shell_contract: 'v1' } }).contract,
    'v2',
    'a stale runtime record must not downgrade tenant shell chrome',
  );
  assert.equal(knowledge.layout.icon_asset, 'modules/knowledge/assets/icon/knowledge-256.png');
  assert.equal(
    knowledge.layout.icon_asset_srcset,
    'modules/knowledge/assets/icon/knowledge-256.png 1x, modules/knowledge/assets/icon/knowledge-512.png 2x, modules/knowledge/assets/icon/knowledge-1024.png 4x',
  );
  assert.notEqual(knowledge.layout.icon_asset, knowledge.layout.icon_asset_60);
  assert.equal(knowledge.layout.shell_header_rows, 2);
  assert.equal(knowledge.layout.shell_icon_rows, 2);
  assert.deepEqual(knowledge.presentation.initial_size, { width: 1200, height: 720 });
  assert.equal(knowledge.layout.icon_svg, undefined);
  assert.match(knowledge.layout.frame_palette.start, /^#[0-9a-f]{6}$/i);
  assert.match(knowledge.layout.frame_palette.top_joint, /^#[0-9a-f]{6}$/i);
  assert.match(knowledge.layout.frame_palette.left_joint, /^#[0-9a-f]{6}$/i);
  assert.ok(
    [
      knowledge.layout.frame_palette.start,
      knowledge.layout.frame_palette.middle,
      knowledge.layout.frame_palette.top_joint,
      knowledge.layout.frame_palette.left_joint,
      knowledge.layout.frame_palette.end,
    ].includes(knowledge.layout.frame_palette.accent),
    'manifest fallback accents must come from the icon-derived frame palette',
  );
  assert.doesNotMatch(appCss, /--shell-v2-accent:\s*#75d7c2/i);
  assert.match(windowManagerSource, /shellV2FramePaletteFromRgba/);
  assert.match(windowManagerSource, /dataset\.shellV2PaletteSource = 'icon'/);
  assert.doesNotMatch(windowManagerSource, /'\[data-shell-v2-accent\]'/, 'module content uses the icon-derived content palette, not spatial frame samples');
  assert.match(windowManagerSource, /--shell-v2-content-accent/);
  assert.match(appCss, /--shell-v2-icon-size:\s*80px/);
  assert.match(appCss, /\.shell-window\[data-shell-contract="v2"\] \.shell-window-control--close::before/);
  assert.match(appCss, /--shell-v2-close-size:\s*44px/);
  assert.match(appCss, /width:\s*18px;[\s\S]*?height:\s*2px;[\s\S]*?background:\s*currentColor/);
  assert.match(appCss, /:root\[data-shell-style\] \.shell-window\[data-shell-contract="v2"\] \.shell-window-control--close:hover/);
  assert.match(appCss, /width:\s*var\(--shell-v2-close-size\)/);
  assert.match(appCss, /@media \(forced-colors: active\)[\s\S]*?shell-window-control--close[\s\S]*?background:\s*Canvas;[\s\S]*?color:\s*CanvasText/);
  assert.doesNotMatch(appCss, /\.desktop-icon\[data-target="knowledge"\][\s\S]*?\.desktop-icon-glyph\s*\{[\s\S]*?width:\s*128px/);
  assert.match(windowManagerSource, /shellV2RenderedIconSizeFromAnchor\(anchor\)/);
  assert.match(windowManagerSource, /setProperty\('--shell-v2-icon-size', `\$\{renderedSize\}px`\)/);
  assert.match(windowManagerSource, /restoreAppDock\(win, \{ failClosed: false \}\)/);
  assert.match(windowManagerSource, /function finalizeDockRestore\(\)/);
  assert.match(windowManagerSource, /bus\.emit\('window:closing', \{ id, ownerId: win\.ownerId \}\)/);
  assert.match(windowManagerSource, /addEventListener\('lostpointercapture', lost\)/);
  assert.match(windowManagerSource, /querySelectorAll\('\[data-window-drag-region\]'\)/);
  assert.match(windowManagerSource, /const interactiveSelector = \[[\s\S]*?'button', 'a', 'input', 'select', 'textarea'/);
  assert.match(windowManagerSource, /closest\?\.\('\[data-shell-v2-header-row="1"\]'\)[\s\S]*?beginDrag\(event, win\.element\)/);
  assert.match(appCss, /--shell-v2-icon-frame-span:\s*calc\(var\(--shell-v2-icon-size\) - 6px\)/);
  assert.match(appCss, /var\(--shell-v2-frame-top-joint\) 0 var\(--shell-v2-icon-frame-span\)/);
  assert.match(windowManagerSource, /addEventListener\('lostpointercapture', finish\)/);
  assert.match(appSource, /state\.windowManager\?\.finalizeDockRestore\?\.\(\)/);
  assert.match(appSource, /loadModules\(\{ timeoutMs: 20000, allowShellSeed: true \}\)/);
  assert.match(appSource, /const width = Number\(glyph\.offsetWidth\) \|\| visualRect\.width/);
  assert.match(appSource, /visualRect\.left \+ \(visualRect\.width - width\) \/ 2/);
  assert.match(appCss, /\.desktop-icon\.is-app-open:hover \.desktop-icon-glyph[\s\S]*?transform:\s*none !important/);
  assert.match(appSource, /shellContract:\s*shell\?\.contract \|\| 'v2'/);
  assert.match(appSource, /iconAsset:\s*String\(operatorIcon\?\.asset \|\| mod\?\.layout\?\.icon_asset/);
  assert.match(appSource, /shellContract:\s*'v2',[\s\S]*?iconAnchorRect:\s*\(\) => desktopIconAnchorRect\(entry\.id\)/);
  assert.match(appSource, /trigger\.className = 'shell-v2-window-title-fallback'/);
  assert.match(windowManagerSource, /options\.shellContract === 'v1' \? 'v1' : 'v2'/);
  assert.match(windowManagerSource, /iconHost\?\.classList\.add\('is-fallback'\)/);
  assert.match(windowManagerSource, /if \(iconEl && options\.iconAsset\)[\s\S]*?else \{[\s\S]*?iconEl\?\.remove\(\)/);
  assert.doesNotMatch(windowManagerSource, /<img[^>]+src=["']\s*["']/);
  assert.match(appSource, /load\(ownerId, \{ shellContract = 'v2', shellGeometryContract = '' \} = \{\}\)/);
  assert.match(appSource, /if \(shellContract === 'v2' && cached\.shell_contract !== 'v2'\) return null/);
  assert.match(appSource, /shell_contract:\s*snapshot\.shellContract \|\| 'v2'/);
  for (const moduleId of ['importer', 'spreadsheets']) {
    assert.ok(visibleWindowModules.some((moduleDef) => moduleDef.id === moduleId));
  }
  assert.match(appSource, /root\.dataset\.resizeFrame = ''/);
  assert.match(appSource, /leftResizer\.dataset\.resizerVar = '--shell-module-left-width'/);
  assert.match(appSource, /rightResizer\.dataset\.resizerVar = '--shell-module-right-width'/);
  assert.match(appSource, /scope:\s*root,[\s\S]*?resizers:\s*windowResizers/);
  assert.match(appSource, /function createModuleDrawerController\(hostEl\)/);
  assert.match(appSource, /closest\?\.\('\.shell-window-module-root'\)/);
  assert.match(appSource, /openLeftDrawer:\s*\(content\) => moduleDrawers\.open\('left', content\)/);
  assert.doesNotMatch(appSource, /openLeftDrawer:\s*\(content\) => openDrawer\('left', content\)/);
  assert.match(appCss, /\.shell-module-drawer-overlay\s*\{[\s\S]*?position:\s*absolute;[\s\S]*?inset:\s*0/);
  assert.match(appCss, /grid-template-columns:[\s\S]*?var\(--shell-module-left-width\)[\s\S]*?var\(--shell-module-right-width\)/);
  assert.match(appCss, /shell-window-module-pane--left:empty[\s\S]*?shell-window-module-column-resizer--left/);
  assert.match(windowManagerSource, /const resizeHandles = shellContract === 'v2' \? V2_RESIZE_HANDLES : RESIZE_HANDLES/);
  assert.match(windowManagerSource, /V2_RESIZE_HANDLES = \['nw', 'ne', 'sw', 'se'\]/);
});

test('Knowledge raster provenance binds the exact 1024 master and every responsive derivative', () => {
  const iconRoot = resolve(businessOsRoot, 'modules/knowledge/assets/icon');
  const provenance = JSON.parse(readFileSync(resolve(iconRoot, 'provenance.json'), 'utf8'));
  const sha256 = (path) => createHash('sha256').update(readFileSync(path)).digest('hex');
  assert.equal(provenance.source.sha256, '6aaaac3c849a444f1bc8af3e4f019eb4eded5ad64ac70ec85eee5c54cdb06e3b');
  assert.equal(sha256(resolve(iconRoot, provenance.source.path)), provenance.source.sha256);
  assert.equal(provenance.source.width, 1024);
  assert.equal(provenance.source.height, 1024);
  assert.equal(provenance.vector, null);
  for (const derivative of provenance.derivatives) {
    assert.equal(sha256(resolve(iconRoot, derivative.path)), derivative.sha256, derivative.path);
  }
  assert.deepEqual(provenance.derivatives.map((entry) => entry.size_px), [1024, 512, 256, 144, 60]);
});

test('legacy, runtime, and imported app records cannot opt back into full-workspace mounting', () => {
  const records = [
    { id: 'legacy-imported', layout: { shell: 'full-workspace' } },
    { id: 'runtime-missing-presentation' },
    { id: 'runtime-windowed', launch_kind: 'desktop-app' },
    { id: 'imported-desktop-window', layout: { shell: 'desktop-window' } },
  ];

  for (const moduleDef of records) {
    assert.equal(launchesInWindow(moduleDef), true, `${moduleDef.id} must be window-hosted`);
    assert.equal(resolvePresentation(moduleDef).defaultMode, 'window');
    assert.equal(usesLegacyWorkspace(moduleDef), false);
  }
});

test('the shared shell keeps v1 chrome while v2 exposes icon drag, four corners and close only', () => {
  assert.deepEqual([...SHELL_WINDOW_CONTROL_ACTIONS].sort(), ['close', 'maximize', 'minimize']);
  assert.equal(SHELL_WINDOW_CHROME_VERSION, 'shared-v1');
  assert.equal(SHELL_WINDOW_V2_CHROME_VERSION, 'shared-v2');
  assert.match(windowManagerSource, /winEl\.dataset\.shellWindow = 'true'/);
  assert.match(windowManagerSource, /winEl\.dataset\.shellWindowChrome = shellContract === 'v2'/);
  assert.match(windowManagerSource, /data-window-header data-window-drag-region/);
  assert.match(windowManagerSource, /data-window-header data-window-drag-region data-shell-v2-header-row="1"/);
  assert.match(windowManagerSource, /class="shell-window-v2-icon" data-window-drag-region/);
  assert.match(windowManagerSource, /data-window-controls data-window-control-strip/);
  assert.match(windowManagerSource, /btn\.dataset\.windowControl = kind/);
  assert.match(windowManagerSource, /querySelector\('\[data-window-drag-region\]'\)/);
  assert.match(windowManagerSource, /windows: SHELL_WINDOW_CONTROL_ACTIONS/);
  assert.match(windowManagerSource, /macos: \['close', 'minimize', 'maximize'\]/);
  assert.match(windowManagerSource, /assertShellWindowChrome\(winEl, shellContract\)/);
  assert.match(windowManagerSource, /assertShellWindowChrome\(win\.element, win\.shellContract\)/);
  assert.match(windowManagerSource, /shellContract === 'v2'\s*\? \['close'\]/);
  assert.match(windowManagerSource, /V2_RESIZE_HANDLES = \['nw', 'ne', 'sw', 'se'\]/);
  assert.match(windowManagerSource, /const finishDestroy = \(\) => \{[\s\S]*?focusNextAfter\(id\);[\s\S]*?window:closed/);
  assert.match(windowManagerSource, /btn\.type = 'button'/);
  assert.match(windowManagerSource, /btn\.setAttribute\('aria-label'/);
  for (const action of SHELL_WINDOW_CONTROL_ACTIONS) {
    assert.match(windowManagerSource, new RegExp(`['"]${action}['"]`));
  }
  assert.match(windowManagerSource, /if \(action === 'close'\) destroy\(win\.id\)/);
  assert.match(windowManagerSource, /else if \(action === 'minimize'\) minimize\(win\.id\)/);
  assert.match(windowManagerSource, /else if \(action === 'maximize'\) toggleMaximize\(win\.id\)/);
  assert.match(windowManagerSource, /setChromeLayout[\s\S]{0,500}renderControls\(/);
});

test('mobile sheets keep one close action while desktop retains the complete window contract', () => {
  assert.match(appCss, /@media \(max-width: 767px\)[\s\S]*\.shell-window-control--minimize,[\s\S]*\.shell-window-control--maximize[\s\S]*display: none/);
  assert.doesNotMatch(appCss, /\.shell-window-control--close[^{}]*\{[^}]*display:\s*none/);
});

test('public shell titles are localized before registry implementation names are exposed', () => {
  assert.match(appSource, /shellText\('moduleTitles'\)\?\.\[mod\.id\]/);
  assert.match(appSource, /documents:\s*'Dokumente'/);
  assert.match(appSource, /documents:\s*'Documents'/);
  assert.match(appSource, /spreadsheets:\s*'Tabellen'/);
  assert.match(appSource, /spreadsheets:\s*'Spreadsheets'/);
});

test('all app launch routes converge on the shared window manager', () => {
  assert.match(appSource, /async function openDesktopApp\(appId, options = \{\}\)/);
  assert.match(appSource, /async function openWindowedModule\(mod, options = \{\}\)/);
  assert.match(appSource, /state\.windowManager\.create\(\{/);
  assert.match(appSource, /ownerId: `desktop-app:\$\{entry\.id\}`/);
  assert.match(appSource, /ownerId: `desktop-app:\$\{mod\.id\}`/);
  // explorer and file-viewer used to be shell-owned static desktop apps; the
  // module port (4ab89e374) turned them into real windowed modules, so they no
  // longer appear as `id: '<app>'` literals in app.js. What must hold is that
  // they still reach the desktop through the shared window manager: a windowed
  // v2 manifest in the generated catalog, launched by `openWindowedModule`.
  for (const portedAppId of ['explorer', 'file-viewer']) {
    assert.match(appSource, new RegExp(`"id": "${portedAppId}"`), `${portedAppId} must stay in the generated offline catalog`);
    const manifest = JSON.parse(readFileSync(new URL(`../modules/${portedAppId}/module.json`, import.meta.url), 'utf8'));
    assert.equal(manifest?.layout?.shell, 'windowed', `${portedAppId} must launch as a window`);
    assert.equal(manifest?.layout?.shell_contract, 'v2', `${portedAppId} must carry the v2 shell contract`);
  }
  assert.doesNotMatch(appSource, /id:\s*'code-editor',[\s\S]*?title:\s*'Source Editor'/);
  assert.match(appSource, /mountIntegratedModuleSource[\s\S]*?desktop-apps\/code-editor\/app\.js/);
  assert.match(appSource, /async function openDesktopApp[\s\S]*?shellContract:\s*'v2',[\s\S]*?state\.windowManager\.create/);
  assert.match(appSource, /if \(moduleLaunchesAsDesktopApp\(mod\)\) \{/);
  assert.doesNotMatch(appSource, /moduleLaunchesAsDesktopApp\(mod\) && !options\.asModule/);
  assert.match(appSource, /Every Business OS app is hosted by the shared window manager/);
});

test('desktop launchers stay desaturated for every open window, including minimized windows', () => {
  const syncOpenIcons = appSource.match(/function syncDesktopOpenIconStates\(\) \{([\s\S]*?)\n\}/)?.[1] || '';
  assert.match(syncOpenIcons, /ownerId\?\.startsWith\('desktop-app:'\)/);
  assert.doesNotMatch(syncOpenIcons, /state\s*!==\s*'minimized'/);
  assert.match(appCss, /\.desktop-icon\.is-app-open \.desktop-icon-glyph[\s\S]*?filter:\s*grayscale\(1\) saturate\(0\)/);
});

test('the global right-click handoff stays a compact shell popover', () => {
  assert.match(appSource, /class="ctox-context-chat-form"/);
  assert.doesNotMatch(appSource, /className = 'ctox-context-menu ctox-global-context-menu'/);
  assert.match(appSource, /className = 'ctox-global-context-menu'/);
  assert.match(appSource, /class="ctox-context-composer" hidden/);
  assert.match(appCss, /\.ctox-global-context-menu\s*\{[\s\S]*?position:\s*fixed[\s\S]*?width:\s*min\(292px, calc\(100vw - 16px\)\)/);
  assert.match(appCss, /\.ctox-context-textarea\s*\{[\s\S]*?min-height:\s*54px[\s\S]*?max-height:\s*104px/);
  assert.match(appCss, /\.ctox-context-mode\s*\{[\s\S]*?grid-template-columns:\s*minmax\(0, 1fr\)/);
  assert.match(appCss, /data-stage="compose"[\s\S]*?grid-template-columns:\s*repeat\(3, minmax\(0, 1fr\)\)/);
});
