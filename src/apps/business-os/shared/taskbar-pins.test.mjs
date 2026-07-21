import assert from 'node:assert/strict';

import {
  decodeTaskbarPinCache,
  encodeTaskbarPinCache,
  resolveTaskbarPinState,
} from './taskbar-pins.js';

const legacy = decodeTaskbarPinCache(JSON.stringify(['files', 'ctox', 'files']));
assert.deepEqual(legacy, { pins: ['files', 'ctox'], updatedAtMs: 0, legacy: true });

const encoded = encodeTaskbarPinCache(['files', 'ctox'], 42);
assert.deepEqual(decodeTaskbarPinCache(encoded), {
  pins: ['files', 'ctox'],
  updatedAtMs: 42,
  legacy: false,
});

assert.deepEqual(resolveTaskbarPinState({
  localPins: ['ctox'],
  localUpdatedAtMs: 200,
  remotePins: ['files'],
  remoteUpdatedAtMs: 100,
}), { pins: ['ctox'], updatedAtMs: 200, source: 'local' });

assert.deepEqual(resolveTaskbarPinState({
  localPins: ['ctox'],
  localUpdatedAtMs: 100,
  remotePins: ['files'],
  remoteUpdatedAtMs: 200,
}), { pins: ['files'], updatedAtMs: 200, source: 'remote' });

console.log('ok - taskbar pins survive reloads and newest-write-wins reconciliation');
