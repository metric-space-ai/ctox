import test from 'node:test';
import assert from 'node:assert/strict';

import {
  launchesInWindow,
  resolvePresentation,
  usesLegacyWorkspace,
} from './presentation.js';

test('presentation contract resolves explicit window configuration', () => {
  const presentation = resolvePresentation({
    presentation: {
      default_mode: 'maximized',
      supported_modes: ['window', 'focus'],
      initial_size: { width: 1280, height: 800 },
      minimum_size: { width: 720, height: 520 },
      multi_instance: true,
    },
  });

  assert.equal(presentation.defaultMode, 'maximized');
  assert.deepEqual(presentation.supportedModes, ['maximized', 'window', 'focus']);
  assert.deepEqual(presentation.initialSize, { width: 1280, height: 800 });
  assert.deepEqual(presentation.minimumSize, { width: 720, height: 520 });
  assert.equal(presentation.multiInstance, true);
});

test('presentation contract retains bounded legacy behavior', () => {
  const windowed = { layout: { shell: 'desktop-window' } };
  const workspace = { layout: { shell: 'full-workspace' } };

  assert.equal(launchesInWindow(windowed), true);
  assert.equal(usesLegacyWorkspace(workspace), true);
});
