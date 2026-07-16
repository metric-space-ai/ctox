import assert from 'node:assert/strict';
import test from 'node:test';

import {
  buildWorkspaceSessionSnapshot,
  normalizeWorkspaceSessionSnapshot,
} from './workspace-session.js';

test('captures restorable app inventory independently from geometry', () => {
  const snapshot = buildWorkspaceSessionSnapshot([
    { ownerId: 'desktop-app:research', state: 'maximized', appMode: 'focus', isFocused: true },
    { ownerId: 'transient:menu', state: 'normal' },
  ], 'knowledge', 42);
  assert.deepEqual(snapshot, {
    version: 1,
    updatedAtMs: 42,
    activeModuleId: 'knowledge',
    windows: [{
      ownerId: 'desktop-app:research',
      state: 'maximized',
      appMode: 'focus',
      focused: true,
    }],
  });
});

test('rejects malformed and duplicate restore entries', () => {
  assert.equal(normalizeWorkspaceSessionSnapshot({ version: 2 }), null);
  const snapshot = normalizeWorkspaceSessionSnapshot({
    version: 1,
    windows: [
      { ownerId: 'desktop-app:files', state: 'minimized' },
      { ownerId: 'desktop-app:files', state: 'maximized' },
      { ownerId: 'module:research', state: 'normal' },
    ],
  });
  assert.deepEqual(snapshot.windows, [{
    ownerId: 'desktop-app:files',
    state: 'minimized',
    appMode: 'window',
    focused: false,
  }]);
});
