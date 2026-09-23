import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import test from 'node:test';

const source = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const helper = source.match(/async function closeWindowForRecovery\(id\) \{[\s\S]*?\n\}/)?.[0] || '';
const retries = [...source.matchAll(/onRetry: async \(\) => \{([\s\S]*?)\n      \},/g)].map(match => match[1]);

function fixture(body, mode = 'delayed') {
  const listeners = new Map();
  const old = { id: 'old', ownerId: 'desktop-app:mail' };
  const windows = [old];
  let mounted = 0;
  const emit = id => { for (const listener of listeners.values()) listener({ id }); };
  const finish = () => { windows.splice(0); emit(old.id); };
  const state = {
    modules: [{ id: 'mail' }],
    eventBus: { on: (_, fn) => { const token = {}; listeners.set(token, fn); return token; }, off: (_, token) => listeners.delete(token) },
    windowManager: {
      listWindows: () => windows,
      destroy: () => {
        if (mode === 'veto') return Promise.resolve(false);
        if (mode === 'error') throw new Error('close failed');
        if (mode === 'immediate') finish();
        return Promise.resolve(true);
      },
    },
  };
  const open = () => { if (!windows.length) mounted++; return 'new'; };
  const retry = runInNewContext(`${helper}\n(async () => {${body}})`, {
    state, win: old, mod: { id: 'mail' }, appId: 'mail', options: {},
    openDesktopApp: open, openWindowedModule: open,
    delay: async () => {},
  });
  return { retry, finish, emit, listeners, mounted: () => mounted };
}

for (const [index, body] of retries.entries()) {
  test(`recovery ${index} waits for registry removal beyond the old timer`, async () => {
    const f = fixture(body);
    let done = false;
    const result = f.retry().then(() => { done = true; });
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(done, false);
    assert.equal(f.mounted(), 0);
    f.emit('unrelated');
    await Promise.resolve();
    assert.equal(done, false);
    f.finish();
    await result;
    assert.equal(f.mounted(), 1);
    assert.equal(f.listeners.size, 0);
  });
  test(`recovery ${index} handles immediate close`, async () => {
    const f = fixture(body, 'immediate');
    await f.retry();
    assert.equal(f.mounted(), 1);
    assert.equal(f.listeners.size, 0);
  });
  for (const mode of ['veto', 'error']) test(`recovery ${index} preserves ${mode}`, async () => {
    const f = fixture(body, mode);
    await assert.rejects(f.retry());
    assert.equal(f.mounted(), 0);
    assert.equal(f.listeners.size, 0);
  });
}
test('both actual shell recovery callbacks are exercised', () => assert.equal(retries.length, 2));
