import test from 'node:test';
import assert from 'node:assert/strict';

import {
  SHELL_CHAT_LAYOUT_EVENT,
  createShellChatCompositionController,
  deriveShellChatInsets,
} from './shell-chat-composition.js';

test('expanded chat remains an overlay and reserves only the visible dock', () => {
  assert.deepEqual(deriveShellChatInsets({
    detail: { present: true, expanded: true, top: 304, dock_top: 640 },
    viewport: { w: 1280, h: 668 },
  }), {
    expanded: true,
    side: false,
    compact: false,
    top: 0,
    right: 0,
    bottom: 36,
    left: 0,
  });
});

test('chat never reserves its expanded conversation height', () => {
  const clamped = deriveShellChatInsets({
    detail: { present: true, expanded: true, top: -100, dock_top: 450 },
    viewport: { w: 800, h: 500 },
  });
  assert.equal(clamped.bottom, 58);
  assert.equal(clamped.right, 0);

  const collapsed = deriveShellChatInsets({
    detail: { present: true, expanded: false, top: 420 },
    viewport: { w: 800, h: 500 },
  });
  assert.deepEqual(collapsed, {
    expanded: false,
    side: false,
    compact: false,
    top: 0,
    right: 0,
    bottom: 0,
    left: 0,
  });
});

test('collapsed chat reserves only its visible dock instead of the full chat root', () => {
  assert.deepEqual(deriveShellChatInsets({
    detail: { present: true, expanded: false, top: 430, dock_top: 778 },
    viewport: { w: 390, h: 844 },
  }), {
    expanded: false,
    side: false,
    compact: false,
    top: 0,
    right: 0,
    bottom: 74,
    left: 0,
  });
});

test('large minimum-height windows never move chat into a side rail', () => {
  const wide = deriveShellChatInsets({
    detail: { present: true, expanded: true, top: 304, height: 398, dock_top: 620 },
    viewport: { w: 1280, h: 668 },
    minimumWorkArea: { width: 640, height: 480 },
    expandedHeightHint: 398,
  });
  assert.deepEqual(wide, {
    expanded: true,
    side: false,
    compact: false,
    top: 0,
    right: 0,
    bottom: 56,
    left: 0,
  });

  const narrow = deriveShellChatInsets({
    detail: { present: true, expanded: true, top: 184, height: 398, dock_top: 500 },
    viewport: { w: 900, h: 548 },
    minimumWorkArea: { width: 640, height: 480 },
    expandedHeightHint: 398,
  });
  assert.equal(narrow.side, false);
  assert.equal(narrow.compact, false);
  assert.equal(narrow.right, 0);
  assert.equal(narrow.bottom, 56);
});

test('composition controller owns event lifecycle, shell state, and transient window insets', () => {
  const listeners = new Map();
  const calls = [];
  const attributes = new Set();
  const properties = new Map();
  const eventTarget = {
    addEventListener(name, fn) { listeners.set(name, fn); },
    removeEventListener(name, fn) { if (listeners.get(name) === fn) listeners.delete(name); },
  };
  const controller = createShellChatCompositionController({
    windowManager: {
      getViewport: () => ({ w: 1280, h: 668 }),
      setInsets: (...args) => calls.push(args),
    },
    eventTarget,
    bodyEl: {
      toggleAttribute(name, enabled) {
        if (enabled) attributes.add(name);
        else attributes.delete(name);
      },
    },
    rootEl: { style: { setProperty: (name, value) => properties.set(name, value) } },
  });

  controller.start();
  assert.equal(typeof listeners.get(SHELL_CHAT_LAYOUT_EVENT), 'function');
  listeners.get(SHELL_CHAT_LAYOUT_EVENT)({ detail: { present: true, expanded: true, top: 304, dock_top: 640 } });
  assert.equal(attributes.has('data-shell-chat-dock-expanded'), true);
  assert.equal(attributes.has('data-shell-chat-dock-side'), false);
  assert.equal(properties.get('--shell-chat-dock-inset-bottom'), '36px');
  assert.deepEqual(calls.at(-1), [
    { top: 0, right: 0, bottom: 36, left: 0 },
    { affectNormal: false, transient: false },
  ]);

  controller.stop();
  assert.equal(listeners.has(SHELL_CHAT_LAYOUT_EVENT), false);
  assert.equal(attributes.has('data-shell-chat-dock-expanded'), false);
  assert.deepEqual(calls.at(-1), [
    { top: 0, right: 0, bottom: 0, left: 0 },
    { affectNormal: false, transient: false },
  ]);
});
