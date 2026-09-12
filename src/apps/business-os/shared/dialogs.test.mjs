import test from 'node:test';
import assert from 'node:assert/strict';

test('dialog imports tolerate Node and track browser interactions once across module versions', async () => {
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, 'window');
  const previousDocument = Object.getOwnPropertyDescriptor(globalThis, 'document');
  try {
    delete globalThis.window;
    delete globalThis.document;
    const dialogs = await import('./dialogs.js?import-test-node');
    assert.equal(typeof dialogs.showBusinessConfirm, 'function');

    globalThis.window = {};
    await import('./dialogs.js?import-test-no-document');
    assert.equal(globalThis.window.__ctoxDialogInteractionTracked, undefined);

    globalThis.document = {};
    await import('./dialogs.js?import-test-partial-document');
    assert.equal(globalThis.window.__ctoxDialogInteractionTracked, undefined);

    const listeners = [];
    globalThis.document = {
      addEventListener(type, listener, capture) {
        listeners.push({ type, listener, capture });
      },
    };
    await import('./dialogs.js?import-test-browser-first');
    await import('./dialogs.js?import-test-browser-second');
    assert.equal(globalThis.window.__ctoxDialogInteractionTracked, true);
    assert.deepEqual(listeners.map(({ type, capture }) => ({ type, capture })), [
      { type: 'pointerdown', capture: true },
      { type: 'focusin', capture: true },
    ]);

    for (const { listener } of listeners) {
      const host = { isConnected: true };
      listener({ target: { closest(selector) {
        assert.equal(selector, '[data-module-content]');
        return host;
      } } });
      assert.equal(globalThis.window.__ctoxLastDialogInteraction, host);
      listener({ target: {} });
      assert.equal(globalThis.window.__ctoxLastDialogInteraction, host);
    }
  } finally {
    if (previousWindow) Object.defineProperty(globalThis, 'window', previousWindow);
    else delete globalThis.window;
    if (previousDocument) Object.defineProperty(globalThis, 'document', previousDocument);
    else delete globalThis.document;
  }
});
