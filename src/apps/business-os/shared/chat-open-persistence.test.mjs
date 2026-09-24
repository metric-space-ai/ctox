import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

globalThis.window ||= globalThis;
globalThis.document ||= { documentElement: { lang: 'en' }, addEventListener() {}, body: { append() {} } };

const { __businessChatTestInternals } = await import('./business-chat.js');
const notifications = [];
const rejected = new Error('permission denied');
const persisted = await __businessChatTestInternals.persistExternalChatOpen(
  async () => { throw rejected; },
  (error) => notifications.push(error.message),
);
assert.equal(persisted, false);
assert.deepEqual(notifications, ['permission denied']);

const previousWarn = console.warn;
console.warn = () => {};
try {
  const remoteError = await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error('remote failure callback did not run')), 2000);
    __businessChatTestInternals.scheduleChatRemotePersistence({
      findOne: () => ({ exec: async () => { throw rejected; } }),
    }, [{ id: 'chat-1' }], (error) => {
      clearTimeout(timeout);
      resolve(error);
    });
  });
  assert.equal(remoteError, rejected);

  let remoteQueryCompleted;
  const queryCompleted = new Promise((resolve) => { remoteQueryCompleted = resolve; });
  let transientNotifications = 0;
  __businessChatTestInternals.scheduleChatRemotePersistence({
    findOne: () => ({ exec: async () => {
      remoteQueryCompleted();
      throw new Error('IDBDatabase is closing');
    } }),
  }, [{ id: 'chat-volatile' }], () => { transientNotifications += 1; });
  await queryCompleted;
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(transientNotifications, 0, 'volatile remote errors retain local-first behavior without a persistence-failure toast');
} finally {
  console.warn = previousWarn;
}

const desktopSource = readFileSync(new URL('../modules/desktop/index.js', import.meta.url), 'utf8');
assert.match(desktopSource, /return openCtoxChat\(detail\)/);
assert.match(desktopSource, /return dispatchDesktopChatOpen\(\{/);

console.log('chat-open persistence failure feedback ok');
