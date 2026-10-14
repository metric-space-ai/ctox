import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import { appLifecycleBadge } from './app-lifecycle.js';

const source = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
// Execute the actual shell functions without booting a second app or database.
function shellFunction(name) {
  const start = source.indexOf(`function ${name}(`);
  assert.ok(start >= 0, `shell function ${name} exists`);
  const rest = source.slice(start);
  const end = rest.slice(1).search(/\n(?:async )?function /);
  assert.ok(end >= 0, `shell function ${name} has a bounded successor`);
  return rest.slice(0, end + 1);
}

for (const updateAvailable of [false, true]) {
  test(`windowed preview app retains its lifecycle through launcher and taskbar (update=${updateAvailable})`, () => {
    const module = {
      id: 'preview-app', title: 'Preview App', version: '0.4.0', source: 'installed',
      lifecycle: { runtime_installed: true, visibility_state: 'preview', audience: 'preview', update_available: updateAvailable },
    };
    const badgeListeners = new Map();
    const badge = { addEventListener: (name, fn) => badgeListeners.set(name, fn) };
    const button = {
      dataset: {}, setAttribute() {}, addEventListener() {},
      querySelector: () => badge,
    };
    let opened = null;
    const context = {
      state: { modules: [module], session: { user: { id: 'preview_target', role: 'admin' } } },
      appLifecycleBadge,
      document: { createElement: () => button },
      resolvePresentation: () => ({ initialSize: {}, minimumSize: {} }),
      resolveShellWindowContract: () => ({ contract: 'v2' }),
      operatorIconFor: () => null, grokShellIconFor: () => null,
      moduleDisplayTitle: mod => mod.title,
      taskbarMarkForModule: () => 'P', workjetCategoryForModule: () => 'imported',
      moduleAppearsInSwitcher: () => false,
      workjetCategoryForTarget: () => 'imported',
      desktopAppIsFocused: () => false, desktopAppIsRunning: () => false,
      applyWorkjetCategory() {}, getRegisteredSvgIcon: () => '',
      escapeHtml: value => String(value), shellText: () => '',
      lifecycleBadgeAriaLabel: (title, lifecycle) => `${title}: ${lifecycle.text}`,
      openAppLifecycleDrawer: mod => { opened = mod; },
    };
    vm.createContext(context);
    vm.runInContext([
      shellFunction('desktopAppDescriptorForModule'),
      shellFunction('listLaunchTargets'),
      shellFunction('launchTargetForId'),
      shellFunction('renderModuleTab'),
      shellFunction('renderStartMenuLifecycleBadge'),
      'function listDesktopApps() { return [desktopAppDescriptorForModule(state.modules[0])]; }',
    ].join('\n'), context);
    const target = context.launchTargetForId(module.id);
    assert.equal(target.kind, 'app', 'the app still launches through the shared window');
    assert.equal(target.module, module, 'the canonical module survives descriptor projection');
    context.renderModuleTab(target, { pinned: true });
    assert.match(button.innerHTML, /Preview App/);
    assert.doesNotMatch(button.innerHTML, /data-app-lifecycle-badge|module-tab-update|Vorschau|Update verfügbar|App-Sichtbarkeit/,
      `taskbar stays quiet when updateAvailable=${updateAvailable}`);
    const menuBadge = context.renderStartMenuLifecycleBadge(target);
    assert.equal(menuBadge, '');
    assert.equal(badgeListeners.size, 0);
    assert.equal(opened, null);
  });
}
