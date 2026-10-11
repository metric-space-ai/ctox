import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

const source = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const openModule = source.match(/async function openModule\(moduleId, options = \{\}\) \{[\s\S]*?\n\}/)?.[0];
assert.ok(openModule, 'the production route dispatcher must exist');
const start = openModule.indexOf('  if (!canSeeModuleForAppVersion(mod)) {');
const end = openModule.indexOf('\n  // Every Business OS app', start);
assert.ok(start >= 0 && end > start);
const denialBranch = openModule.slice(start, end);

for (const fallbackId of ['desktop', null]) {
  test(`hidden route preserves its denial after lazy dialogs and fallback ${fallbackId}`, async () => {
    const events = [];
    let status = '';
    let hash = 'hidden-app';
    const state = { session: {}, governance: {} };
    const workspaceStatusSource = source.match(/function setWorkspaceStatus\(\) \{[\s\S]*?\n\}/)?.[0];
    assert.ok(workspaceStatusSource);
    const setWorkspaceStatus = vm.runInNewContext(`${workspaceStatusSource}\nsetWorkspaceStatus`, {
      state, setStatus: value => { status = value; }, workspaceStatusText: () => 'Lokaler Workspace',
      renderShellInstanceStatus() {},
    });
    const mod = { id: 'hidden-app', title: 'Hidden app' };
    const denyRoute = vm.runInNewContext(`(async (mod, options) => { ${denialBranch} })`, {
      state,
      canSeeModuleForAppVersion: () => false,
      appLifecycleState: () => ({ reason: 'Preview audience only' }),
      visibleModuleFallbackId: () => fallbackId,
      moduleDisplayTitle: item => item.title,
      setStatus: value => { status = value; },
      async loadShellDialogsModule() {
        await Promise.resolve();
        status = 'Lokaler Workspace';
        return { showBusinessAlert: message => events.push(['alert', message]) };
      },
      shellLang: () => 'de',
      currentHashModuleId: () => hash,
      replaceModuleHash: id => { hash = id; },
      async openModule(id, options) {
        assert.equal(id, 'desktop', 'a denied app must never mount');
        assert.equal(options.isNavHistory, true);
        assert.equal(options.force, true);
        await Promise.resolve();
        status = 'Lokaler Workspace';
        events.push(['fallback', id]);
      },
    });
    await denyRoute(mod, { force: true });
    assert.match(status, /Hidden app.*nicht sichtbar.*Preview audience only/);
    setWorkspaceStatus();
    assert.match(status, /Hidden app.*nicht sichtbar.*Preview audience only/,
      'late bootstrap completion must preserve the denied route reason');
    const visibleRoute = vm.runInNewContext(`(async (mod, options) => { ${denialBranch} })`, {
      state, canSeeModuleForAppVersion: () => true,
    });
    await visibleRoute({ id: 'desktop' }, {});
    setWorkspaceStatus();
    assert.equal(status, 'Lokaler Workspace', 'a subsequent allowed route clears the denial');
    assert.equal(hash, fallbackId || 'hidden-app');
    assert.equal(events[0][0], 'alert');
    assert.equal(events.filter(event => event[0] === 'fallback').length, fallbackId ? 1 : 0);
  });
}
