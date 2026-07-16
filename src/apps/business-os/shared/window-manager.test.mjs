import assert from 'node:assert/strict';
import test from 'node:test';

import { clampNormalWindowPosition } from './window-manager.js';

const viewport = {
  w: 1200,
  h: 900,
  left: 0,
  right: 0,
  top: 52,
  bottom: 70,
};

test('keeps a normal window completely inside the shell work area', () => {
  assert.deepEqual(clampNormalWindowPosition({
    left: 459,
    top: 240,
    width: 1200,
    height: 778,
  }, viewport), {
    left: 0,
    top: 52,
  });
});

test('clamps free dragging at every work-area edge', () => {
  assert.deepEqual(clampNormalWindowPosition({
    left: -400,
    top: -100,
    width: 640,
    height: 500,
  }, viewport), {
    left: 0,
    top: 52,
  });
  assert.deepEqual(clampNormalWindowPosition({
    left: 900,
    top: 700,
    width: 640,
    height: 500,
  }, viewport), {
    left: 560,
    top: 330,
  });
});

test('respects shell insets such as a visible side panel or chat rail', () => {
  assert.deepEqual(clampNormalWindowPosition({
    left: 900,
    top: 600,
    width: 700,
    height: 500,
  }, {
    ...viewport,
    left: 24,
    right: 280,
    bottom: 90,
  }), {
    left: 220,
    top: 310,
  });
});
