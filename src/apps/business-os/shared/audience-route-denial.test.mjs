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
    const mod = { id: 'hidden-app', title: 'Hidden app' };
    const denyRoute = vm.runInNewContext(`(async (mod, options) => { ${denialBranch} })`, {
      state: { session: {}, governance: {} },
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
    assert.equal(hash, fallbackId || 'hidden-app');
    assert.equal(events[0][0], 'alert');
    assert.equal(events.filter(event => event[0] === 'fallback').length, fallbackId ? 1 : 0);
  });
}
